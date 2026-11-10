// **A mesa mínima de texto da 1.0, na tela**: editar, responder, ser chamado,
// o autocompletar do `@`, as não lidas de cada canal e a lista de banidos.
//
// A UI é a real (`ui/` como está, com a CSP do produto); a ponte é um dublê
// **com estado**: ele guarda a conversa, os canais e os banidos, e responde
// aos comandos como o core responde — a edição muda o corpo e marca
// «editada», a resposta nasce com `replies_to`, o corpo vazio volta
// `MensagemVazia`, e um servidor 12 volta `ServidorAntigo`. O que se mede é o
// que a pessoa vê e o que sai pela ponte, e não o texto de uma função.
//
// # O que ela prende, no computador (Chromium e WebKit) e no celular
//
//  1. EDITAR só nas mensagens próprias e gravadas; o campo entra em modo de
//     edição com a faixa dizendo isso; Esc devolve o rascunho; Enter salva
//     pelo `editar_mensagem`; a marca «editada» aparece; a seta para cima no
//     campo vazio edita a última própria; o corpo vazio e o servidor antigo
//     têm frase à vista; e num servidor 12 o EDITAR não existe.
//  2. RESPONDER em toda mensagem gravada; a faixa diz a quem; o envio leva
//     `repliesTo`; a resposta mostra a citação da original, e «mensagem
//     anterior» quando ela não está carregada; o clique na citação rola até a
//     original.
//  3. A mensagem que chama é destacada **e** dita («chamou você»); o aviso
//     `Mentioned` aparece escrito; o som de mensagem toca, e não toca com o
//     grupo desligado nas preferências.
//  4. O `@` abre a lista de pessoas, filtrada; setas, Enter, Tab e Esc; os
//     papéis e os `aria-*`; homônimos entram com a marca.
//  5. A lista de canais mostra a conta, com o `@` de quem foi chamado, e a
//     conta some quando o canal abre.
//  6. BANIDOS só para quem pode banir e num servidor 13: pede a lista ao
//     abrir, mostra quem, por quê e até quando, o estado vazio tem frase, e o
//     DESBANIR passa pela confirmação com a consequência escrita.
//
//   PLAYWRIGHT=<playwright-core> node apps/seele-app/bancada/mesa-de-texto.cjs
//
// Com `SEELE_PREVIEW_DIR`, grava as capturas de cada estado nas três medidas.

const assert = require("node:assert/strict");
const path = require("node:path");
const { servir, respostas, retrato } = require("./telas.cjs");
const motores = require("./playwright.cjs");

const agora = Math.floor(Date.now() / 1000);
const pessoa = (id, nickname, extra = {}) => ({
  id, nickname, speaking: false, muted: false, total_isolation: false, signal: 90, sync_band: "Nominal", is_self: false, ...extra,
});

/** A conversa de partida: uma resposta que chama, uma que cita o que não está, e uma pendente. */
function conversa() {
  const m = (id, author, nome, corpo, extra = {}) => ({
    id, channel: 1, author, author_nickname: nome, at_seconds: agora - 3000 + id * 200, body: corpo,
    own: author === 1, edited: false, replies_to: null, mentions_me: false, attachment: null,
    estado: "Confirmada", client_message_id: null, ...extra,
  });
  return [
    m(1, 2, "Lia#K7Q2", "Cheguei. O túnel do norte está fechado até amanhã — o desvio antigo continua aberto, mas some no escuro."),
    m(2, 1, "Alex#A1B2", "Combinado. Levo o desvio antigo."),
    m(3, 2, "Lia#K7Q2", "Boa. @Alex, leva a lanterna grande.", { replies_to: 2, mentions_me: true }),
    m(4, 3, "Rafa#R4F4", "isso ainda vale para sábado?", { replies_to: 99 }),
    m(5, 1, "Alex#A1B2", "Levo. E as pilhas.", { replies_to: 3 }),
    m(0, 1, "Alex#A1B2", "saindo agora", { estado: "Enviando", client_message_id: "k1", at_seconds: agora - 5 }),
  ];
}

function estadoInicial(protocolo) {
  return {
    protocolo,
    mensagens: conversa(),
    proximoId: 6,
    banidos: [
      { person: 21, nickname: "kaworu#A001", reason: "spam de convites", expires_at: null, issued_by: 1 },
      { person: 22, nickname: "toji#B002", reason: null, expires_at: agora + 3 * 86400, issued_by: 1 },
    ],
    foraDoAr: [{ id: 5, nickname: "Marina#M4R1" }],
    retrato: retrato({
      protocolo,
      me: 1,
      presentes: [
        pessoa(1, "Alex#A1B2", { is_self: true }),
        pessoa(2, "Lia#K7Q2"),
        pessoa(3, "Rafa#R4F4"),
        pessoa(4, "Lia#Z9Z9"),
      ],
      identidades_apresentadas: {
        1: ["Alex", "A1B2"], 2: ["Lia", "K7Q2"], 3: ["Rafa", "R4F4"], 4: ["Lia", "Z9Z9"], 5: ["Marina", "M4R1"],
        21: ["kaworu", "A001"], 22: ["toji", "B002"],
      },
      channels: [
        { id: 1, name: "geral", open: true, nao_lidas: 0, mencoes: 0 },
        { id: 2, name: "combinados", open: false, nao_lidas: 3, mencoes: 0 },
        { id: 3, name: "avisos", open: false, nao_lidas: 5, mencoes: 2 },
      ],
      messages_revision: 1,
      fora_do_ar_revision: 1,
    }),
  };
}

/** A ponte de mentira, dentro da página. Tudo o que ela sabe está em `window.mesa`. */
function instalarAPonte({ estado, tabela, celular }) {
  if (celular) {
    new MutationObserver((_, o) => {
      if (!document.documentElement) return;
      document.documentElement.dataset.casca = "celular";
      o.disconnect();
    }).observe(document, { childList: true });
  }
  const ouvintes = new Map();
  const mesa = { ...estado, chamadas: [], osciladores: 0, falhar: null };
  window.mesa = mesa;
  const emitir = (payload) => {
    for (const fn of ouvintes.get("seele://event") ?? []) fn({ payload });
  };
  mesa.emitir = emitir;
  const mexeu = () => {
    mesa.retrato.messages_revision += 1;
    emitir("MessagesChanged");
  };
  const bytes = (texto) => new TextEncoder().encode(texto).length;
  const comandos = {
    snapshot: () => structuredClone(mesa.retrato),
    messages: () => structuredClone(mesa.mensagens),
    membros_fora_do_ar: () => structuredClone(mesa.foraDoAr),
    limite_da_mensagem: () => 4096,
    open_channel: ({ channel }) => {
      for (const canal of mesa.retrato.channels) {
        canal.open = canal.id === channel;
        if (canal.open) Object.assign(canal, { nao_lidas: 0, mencoes: 0 });
      }
      emitir("ChannelsChanged");
      return null;
    },
    send_message: ({ channel, body, repliesTo }) => {
      const id = mesa.proximoId++;
      mesa.mensagens = mesa.mensagens.filter((m) => m.id !== 0);
      mesa.mensagens.push({
        id, channel, author: 1, author_nickname: "Alex#A1B2", at_seconds: Math.floor(Date.now() / 1000),
        body, own: true, edited: false, replies_to: repliesTo ?? null, mentions_me: false, attachment: null,
        estado: "Confirmada", client_message_id: `c${id}`,
      });
      mexeu();
      return `c${id}`;
    },
    editar_mensagem: ({ message, body }) => {
      if (mesa.protocolo < 13) throw { ServidorAntigo: { negociado: mesa.protocolo, exige: 13 } };
      const corpo = String(body).trim();
      if (corpo === "") throw "MensagemVazia";
      if (bytes(corpo) > 4096) throw { MensagemGrandeDemais: { limite_bytes: 4096 } };
      const alvo = mesa.mensagens.find((m) => m.id === message);
      if (!alvo) throw "UnknownMessage";
      alvo.body = corpo;
      alvo.edited = true;
      mexeu();
      return null;
    },
    remover_mensagem: ({ message }) => {
      mesa.mensagens = mesa.mensagens.filter((m) => m.id !== message);
      mexeu();
      return null;
    },
    listar_banidos: () => {
      if (mesa.protocolo < 13) throw { ServidorAntigo: { negociado: mesa.protocolo, exige: 13 } };
      if (!mesa.listaMuda) setTimeout(() => emitir("BanidosChanged"), 30);
      return null;
    },
    banidos: () => structuredClone(mesa.banidos),
    desbanir_pessoa: ({ person }) => {
      mesa.banidos = mesa.banidos.filter((b) => b.person !== person);
      setTimeout(() => emitir("BanidosChanged"), 30);
      return null;
    },
  };
  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        mesa.chamadas.push({ cmd, args: args ? structuredClone(args) : null });
        if (mesa.falhar && Object.hasOwn(mesa.falhar, cmd)) throw mesa.falhar[cmd];
        if (Object.hasOwn(comandos, cmd)) return comandos[cmd](args ?? {});
        return Object.hasOwn(tabela, cmd) ? structuredClone(tabela[cmd]) : null;
      },
    },
    event: {
      listen: async (nome, fn) => {
        if (!ouvintes.has(nome)) ouvintes.set(nome, new Set());
        ouvintes.get(nome).add(fn);
        return () => ouvintes.get(nome).delete(fn);
      },
    },
    window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
  };
  // O som: cada nota é um oscilador, e contá-los é ouvir sem alto-falante.
  for (const Classe of [window.AudioContext, window.webkitAudioContext]) {
    if (!Classe) continue;
    const criar = Classe.prototype.createOscillator;
    Classe.prototype.createOscillator = function (...args) {
      mesa.osciladores += 1;
      return criar.apply(this, args);
    };
  }
}

async function abrir(navegador, servidor, { protocolo = 13, celular = false, largura = 1280, altura = 800 } = {}) {
  const contexto = await navegador.newContext({
    viewport: { width: largura, height: altura }, hasTouch: celular, isMobile: celular, deviceScaleFactor: 1,
  });
  const pagina = await contexto.newPage();
  pagina.setDefaultTimeout(8000);
  const erros = [];
  pagina.on("pageerror", (e) => erros.push(e.message));
  await pagina.addInitScript(instalarAPonte, { estado: estadoInicial(protocolo), tabela: respostas(), celular });
  await pagina.goto(`http://127.0.0.1:${servidor.address().port}/`, { waitUntil: "domcontentloaded" });
  await pagina.waitForTimeout(700);
  await pagina.evaluate(async () => {
    for (const s of document.querySelectorAll("section.tela")) s.hidden = true;
    document.getElementById("tela-sessao").hidden = false;
    if (document.documentElement.dataset.casca === "celular") document.getElementById("tela-sessao").dataset.aba = "conversa";
    linhaAberta = 1;
    await atualizar();
  });
  await pagina.waitForFunction(() => document.querySelectorAll("#lista-mensagens > li[data-mensagem]").length === 6);
  paginaEmCurso = pagina;
  return { pagina, contexto, erros };
}

/** A página do caso que está rodando, para a captura de uma reprovação. */
let paginaEmCurso = null;

const chamadasDe = (pagina, cmd) => pagina.evaluate((cmd) => mesa.chamadas.filter((c) => c.cmd === cmd).map((c) => c.args), cmd);
const visivel = (pagina, seletor) => pagina.locator(seletor).first().evaluate((el) => {
  const r = el.getBoundingClientRect();
  const estilo = getComputedStyle(el);
  return !el.closest("[hidden]") && r.width > 0 && r.height > 0 && estilo.visibility !== "hidden" && estilo.opacity !== "0";
}).catch(() => false);
const texto = (pagina, seletor) => pagina.locator(seletor).first().evaluate((el) => el.textContent.trim());
const mensagem = (id) => `#lista-mensagens > li[data-mensagem-id="${id}"]`;

/** Uma captura, quando `SEELE_PREVIEW_DIR` está posta. */
async function captura(pagina, nome) {
  const pasta = process.env.SEELE_PREVIEW_DIR;
  if (!pasta) return;
  await pagina.waitForTimeout(150);
  await pagina.screenshot({ path: path.join(pasta, `${nome}.png`) });
}

// ------------------------------------------------------------ o computador

async function noComputador(navegador, servidor, motor, medida) {
  const [largura, altura] = medida;
  const sufixo = `${motor}-${largura}x${altura}`;
  const { pagina, contexto, erros } = await abrir(navegador, servidor, { largura, altura });
  const campo = pagina.locator("#campo-mensagem");

  // ---- 5. as não lidas de cada canal
  const canal = (id) => `#lista-linhas button[data-linha="${id}"]`;
  if (largura < 900) await pagina.evaluate(() => alternarCanais(true));
  assert.equal(await pagina.locator(`${canal(1)} .linha-nao-lidas`).count(), 0, "o canal aberto mostra não lidas");
  assert.equal(await texto(pagina, `${canal(2)} .linha-nao-lidas-conta`), "3");
  assert.equal(await texto(pagina, `${canal(2)} .linha-nao-lidas-frase`), "3 mensagens não lidas");
  assert.equal(await texto(pagina, `${canal(3)} .linha-nao-lidas-conta`), "@ 5", "a conta de quem foi chamado não traz o @");
  assert.equal(await texto(pagina, `${canal(3)} .linha-nao-lidas-frase`), "5 mensagens não lidas, 2 chamam você");
  assert.equal(await pagina.locator(`${canal(3)} .linha-nao-lidas`).evaluate((el) => el.classList.contains("com-chamada")), true);
  assert.equal(await visivel(pagina, `${canal(3)} .linha-nao-lidas-conta`), true, "a conta não está à vista");
  const fundos = await pagina.evaluate(() => [2, 3].map((id) => getComputedStyle(document.querySelector(`#lista-linhas button[data-linha="${id}"] .linha-nao-lidas`)).backgroundColor));
  assert.notEqual(fundos[0], fundos[1], "a conta com chamada tem o mesmo fundo da comum");
  await captura(pagina, `canais-nao-lidas-${sufixo}`);
  if (largura < 900) await pagina.evaluate(() => alternarCanais(false));

  // ---- 3. a mensagem que chama: destaque, palavra e aviso
  assert.equal(await pagina.locator(`${mensagem(3)}.mensagem-chamou`).count(), 1, "a mensagem que chama não foi destacada");
  assert.equal(await texto(pagina, `${mensagem(3)} .mensagem-chamou-marca`), "chamou você");
  assert.equal(await pagina.locator(`${mensagem(1)}.mensagem-chamou`).count(), 0);
  const destaque = await pagina.locator(mensagem(3)).evaluate((el) => getComputedStyle(el).backgroundColor);
  assert.notEqual(destaque, "rgba(0, 0, 0, 0)", "a mensagem que chama não tem fundo");

  // ---- 2. a citação de uma resposta
  assert.equal(await texto(pagina, `${mensagem(3)} .mensagem-citacao-autor`), "Alex");
  assert.equal(await texto(pagina, `${mensagem(3)} .mensagem-citacao-corpo`), "Combinado. Levo o desvio antigo.");
  assert.equal(await texto(pagina, `${mensagem(4)} .mensagem-citacao-ausente`), "mensagem anterior");
  assert.equal(await pagina.locator(`${mensagem(1)} :is(.mensagem-citacao, .mensagem-citacao-ausente)`).count(), 0);
  // Uma fileira só, cortada: a citação nunca é mais alta que uma fileira de texto.
  const alturaDaCitacao = await pagina.locator(`${mensagem(3)} .mensagem-citacao`).evaluate((el) => el.getBoundingClientRect().height);
  assert.ok(alturaDaCitacao > 0 && alturaDaCitacao <= 30, `a citação ocupa mais de uma fileira (${alturaDaCitacao}px)`);
  // Acima do que a mensagem diz.
  const ordem = await pagina.locator(mensagem(3)).evaluate((el) => {
    const citacao = el.querySelector(".mensagem-citacao").getBoundingClientRect();
    const corpo = el.querySelector(".corpo").getBoundingClientRect();
    return citacao.bottom <= corpo.top + 1;
  });
  assert.equal(ordem, true, "a citação não está acima do corpo");
  await captura(pagina, `resposta-e-mencao-${sufixo}`);

  // O clique na citação rola até a original e a acende.
  await pagina.evaluate(() => { document.getElementById("lista-mensagens").scrollTop = 1e6; });
  await pagina.locator(`${mensagem(5)} .mensagem-citacao`).click();
  assert.equal(await pagina.locator(`${mensagem(3)}.mensagem-apontada`).count(), 1, "a original não foi acesa");
  const naVista = await pagina.locator(mensagem(3)).evaluate((el) => {
    const lista = document.getElementById("lista-mensagens").getBoundingClientRect();
    const r = el.getBoundingClientRect();
    return r.top >= lista.top - 1 && r.bottom <= lista.bottom + 1;
  });
  assert.equal(naVista, true, "a citação não rolou até a original");
  // Acesa por um instante, e não para sempre.
  await pagina.waitForFunction(() => !document.querySelector("#lista-mensagens > li.mensagem-apontada"), null, { timeout: 4000 });

  // ---- as ações: responder em toda gravada, editar só na própria gravada
  for (const id of [1, 2, 3, 4, 5]) {
    assert.equal(await pagina.locator(`${mensagem(id)} button[data-responder]`).count(), 1, `sem RESPONDER na mensagem ${id}`);
  }
  for (const [id, tem] of [[1, 0], [2, 1], [3, 0], [4, 0], [5, 1]]) {
    assert.equal(await pagina.locator(`${mensagem(id)} button[data-editar]`).count(), tem, `EDITAR na mensagem ${id}`);
  }
  const pendente = "#lista-mensagens > li[data-mensagem]:not([data-mensagem-id])";
  assert.equal(await pagina.locator(pendente).count(), 1);
  assert.equal(await pagina.locator(`${pendente} :is(button[data-responder], button[data-editar])`).count(), 0, "a pendente oferece responder ou editar");
  // Pelo teclado: os três recebem o foco (um botão com `visibility: hidden`
  // não recebe, e o Tab passa por cima dele), e o foco revela o grupo inteiro.
  await pagina.mouse.move(2, 2);
  await pagina.evaluate(() => document.getElementById("campo-mensagem").focus());
  assert.equal(await visivel(pagina, `${mensagem(2)} button[data-editar]`), false, "as ações estão à vista sem ponteiro nem foco");
  // Escondidas, não mexem no texto: a primeira fileira vai até a borda do bloco.
  const fileira = await pagina.locator(`${mensagem(1)} .corpo`).evaluate((el) => {
    const bloco = el.closest(".mensagem-conteudo").getBoundingClientRect();
    const fim = Math.max(...[...el.getClientRects()].map((r) => r.right));
    return { sobra: Math.round(bloco.right - fim), largura: Math.round(bloco.width) };
  });
  assert.ok(fileira.sobra < 90, `as ações escondidas encurtam a fileira do texto: ${JSON.stringify(fileira)}`);
  for (const acao of ["data-responder", "data-editar", "data-moderar-mensagem"]) {
    await pagina.locator(`${mensagem(2)} button[${acao}]`).focus();
    assert.equal(
      await pagina.evaluate((acao) => document.activeElement?.hasAttribute(acao) ?? false, acao),
      true,
      `o botão [${acao}] não recebe o foco do teclado`,
    );
    for (const outra of ["data-responder", "data-editar", "data-moderar-mensagem"]) {
      assert.equal(await visivel(pagina, `${mensagem(2)} button[${outra}]`), true, `o foco em [${acao}] não revela [${outra}]`);
    }
  }

  // ---- 2. responder: a faixa, o cancelar, e o `repliesTo` no envio
  await campo.fill("um rascunho");
  await pagina.locator(`${mensagem(1)} button[data-responder]`).click();
  assert.equal(await visivel(pagina, "#compor-modo"), true);
  assert.equal(await pagina.locator("#compor-modo").getAttribute("data-modo"), "responder");
  assert.equal(await texto(pagina, "#compor-modo-frase"), "Respondendo a Lia");
  assert.match(await texto(pagina, "#compor-modo-citacao"), /^Cheguei\. O túnel do norte/);
  assert.equal(await campo.inputValue(), "um rascunho", "responder apagou o que estava escrito");
  assert.equal(await pagina.evaluate(() => document.activeElement?.id), "campo-mensagem");
  // A faixa não custa a conversa: na janela mínima ainda se lê a que se responde.
  const sobrou = await pagina.evaluate(() => document.getElementById("lista-mensagens").clientHeight);
  assert.ok(sobrou >= 70, `com a faixa de resposta a conversa ficou com ${sobrou}px de altura (${sufixo})`);
  await captura(pagina, `respondendo-${sufixo}`);
  await pagina.locator("#compor-modo-cancelar").click();
  assert.equal(await visivel(pagina, "#compor-modo"), false, "o botão não cancelou a resposta");
  await pagina.locator(`${mensagem(1)} button[data-responder]`).click();
  await campo.press("Escape");
  assert.equal(await visivel(pagina, "#compor-modo"), false, "o Esc não cancelou a resposta");
  assert.equal(await campo.inputValue(), "um rascunho");
  await pagina.locator(`${mensagem(1)} button[data-responder]`).click();
  await campo.fill("vou pelo desvio então");
  await campo.press("Enter");
  await pagina.waitForFunction(() => mesa.chamadas.some((c) => c.cmd === "send_message"));
  assert.deepEqual(await chamadasDe(pagina, "send_message"), [{ channel: 1, body: "vou pelo desvio então", repliesTo: 1 }]);
  assert.equal(await visivel(pagina, "#compor-modo"), false, "a faixa ficou depois do envio");
  await pagina.waitForSelector(mensagem(6));
  assert.equal(await texto(pagina, `${mensagem(6)} .mensagem-citacao-autor`), "Lia");
  // A seguinte, sem resposta, não leva a chave.
  await campo.fill("e esta vai solta");
  await campo.press("Enter");
  await pagina.waitForSelector(mensagem(7));
  assert.deepEqual((await chamadasDe(pagina, "send_message"))[1], { channel: 1, body: "e esta vai solta" });

  // ---- 1. editar
  await campo.fill("rascunho de antes");
  await pagina.locator(`${mensagem(5)} button[data-editar]`).click();
  assert.equal(await pagina.locator("#compor-modo").getAttribute("data-modo"), "editar");
  assert.equal(await texto(pagina, "#compor-modo-frase"), "Editando a sua mensagem");
  assert.equal(await campo.inputValue(), "Levo. E as pilhas.");
  assert.equal(await pagina.locator("#compor-modo-cancelar").getAttribute("title"), "cancelar a edição (Esc)");
  await captura(pagina, `editando-${sufixo}`);
  await campo.press("Escape");
  assert.equal(await visivel(pagina, "#compor-modo"), false, "o Esc não cancelou a edição");
  assert.equal(await campo.inputValue(), "rascunho de antes", "cancelar a edição não devolveu o rascunho");
  assert.deepEqual(await chamadasDe(pagina, "editar_mensagem"), []);

  // O corpo vazio: a frase fica na faixa, e o modo continua.
  await pagina.locator(`${mensagem(5)} button[data-editar]`).click();
  await campo.fill("   ");
  await campo.press("Enter");
  await pagina.waitForFunction(() => !document.getElementById("compor-modo-erro").hidden);
  assert.match(await texto(pagina, "#compor-modo-erro"), /A edição não foi salva\. A MENSAGEM FICOU VAZIA/);
  assert.equal(await pagina.locator("#compor-modo").getAttribute("data-modo"), "editar", "a recusa tirou a pessoa da edição");
  // O corpo grande demais.
  await campo.fill("á".repeat(3000));
  await campo.press("Enter");
  await pagina.waitForFunction(() => /LONGA DEMAIS/.test(document.getElementById("compor-modo-erro").textContent));
  // O aviso de tamanho não toma o lugar do campo: o texto que ele pede para
  // cortar continua ao alcance (o campo ficava com 0px de largura).
  const larguraDoCampo = await campo.evaluate((el) => el.getBoundingClientRect().width);
  assert.ok(larguraDoCampo >= 200, `com o aviso de tamanho o campo ficou com ${larguraDoCampo}px de largura (${sufixo})`);
  // E o servidor que recusa por ser antigo, pela frase dele.
  await pagina.evaluate(() => { mesa.falhar = { editar_mensagem: { ServidorAntigo: { negociado: 12, exige: 13 } } }; });
  await campo.fill("outra coisa");
  await campo.press("Enter");
  await pagina.waitForFunction(() => /DE ANTES DA 1\.0/.test(document.getElementById("compor-modo-erro").textContent));
  assert.doesNotMatch(await texto(pagina, "#compor-modo-erro"), /ALGO FALHOU|ServidorAntigo/);
  await pagina.evaluate(() => { mesa.falhar = null; });

  // A edição que sai: o comando, a marca «editada», e o rascunho de volta.
  await campo.fill("Levo. E as pilhas novas.");
  await campo.press("Enter");
  await pagina.waitForSelector(`${mensagem(5)} .editada`);
  const edicoes = await chamadasDe(pagina, "editar_mensagem");
  assert.deepEqual(edicoes.at(-1), { message: 5, body: "Levo. E as pilhas novas." });
  assert.match(await texto(pagina, `${mensagem(5)} .corpo`), /pilhas novas/);
  assert.equal(await visivel(pagina, "#compor-modo"), false);
  assert.equal(await campo.inputValue(), "rascunho de antes", "salvar a edição não devolveu o rascunho");

  // A seta para cima no campo vazio edita a última própria; com texto, não.
  await campo.press("ArrowUp");
  assert.equal(await visivel(pagina, "#compor-modo"), false, "a seta para cima editou com texto no campo");
  await campo.fill("");
  await campo.press("ArrowUp");
  assert.equal(await pagina.locator("#compor-modo").getAttribute("data-modo"), "editar");
  assert.equal(await campo.inputValue(), "e esta vai solta", "a seta para cima não pegou a última própria");
  // Sem mudança, o Enter não pede nada ao servidor.
  const antes = (await chamadasDe(pagina, "editar_mensagem")).length;
  await campo.press("Enter");
  assert.equal(await visivel(pagina, "#compor-modo"), false);
  assert.equal((await chamadasDe(pagina, "editar_mensagem")).length, antes, "uma edição sem mudança foi ao servidor");

  // Trocar de canal larga o modo, e o corpo editado não vira rascunho de ninguém.
  await campo.fill("rascunho do geral");
  await pagina.locator(`${mensagem(5)} button[data-editar]`).click();
  if (largura < 900) await pagina.evaluate(() => alternarCanais(true));
  await pagina.locator(canal(2)).click();
  await pagina.waitForFunction(() => document.querySelector('#lista-linhas button[data-linha="2"]').classList.contains("aberto"));
  assert.equal(await visivel(pagina, "#compor-modo"), false, "a edição seguiu para o outro canal");
  assert.equal(await campo.inputValue(), "");
  // ---- 5. a conta some ao abrir o canal
  assert.equal(await pagina.locator(`${canal(2)} .linha-nao-lidas`).count(), 0, "a conta ficou no canal aberto");
  if (largura < 900) await pagina.evaluate(() => alternarCanais(true));
  await pagina.locator(canal(1)).click();
  await pagina.waitForFunction(() => document.querySelector('#lista-linhas button[data-linha="1"]').classList.contains("aberto"));
  assert.equal(await campo.inputValue(), "rascunho do geral", "o rascunho do canal se perdeu com a edição");
  await campo.fill("");

  // A mensagem que se editava sai da conversa: a edição é largada, e dito.
  await pagina.locator(`${mensagem(2)} button[data-editar]`).click();
  await pagina.evaluate(() => {
    mesa.mensagens = mesa.mensagens.filter((m) => m.id !== 2);
    mesa.retrato.messages_revision += 1;
    return atualizar();
  });
  await pagina.waitForFunction(() => !document.querySelector('#lista-mensagens > li[data-mensagem-id="2"]'));
  assert.match(await texto(pagina, "#compor-modo-erro"), /saiu da conversa/);
  assert.equal(await campo.inputValue(), "", "o corpo da mensagem removida ficou no campo");
  // E a resposta a ela passa a dizer «mensagem anterior».
  assert.equal(await texto(pagina, `${mensagem(3)} .mensagem-citacao-ausente`), "mensagem anterior");
  await pagina.locator("#compor-modo-cancelar").click();
  assert.equal(await visivel(pagina, "#compor-modo"), false);

  // ---- 4. o autocompletar do `@`
  assert.equal(await campo.getAttribute("aria-controls"), "mencoes-lista");
  assert.equal(await campo.getAttribute("aria-autocomplete"), "list");
  assert.equal(await pagina.locator("#mencoes-lista").getAttribute("role"), "listbox");
  await campo.pressSequentially("oi @");
  await pagina.waitForSelector("#mencoes-lista:not([hidden]) li[role=option]");
  const todas = await pagina.locator("#mencoes-lista li.mencoes-opcao .mencoes-nome").allTextContents();
  // Quem está aqui antes de quem está fora do ar; os homônimos com a marca; nunca a própria pessoa.
  assert.deepEqual(todas, ["@Lia#K7Q2", "@Lia#Z9Z9", "@Rafa", "@Marina"]);
  assert.match(await texto(pagina, "#mencoes-lista li:last-child"), /fora do ar/);
  assert.equal(await pagina.locator("#mencoes-lista li[aria-selected=true]").count(), 1);
  assert.equal(await campo.getAttribute("aria-activedescendant"), "mencao-0");
  await captura(pagina, `autocompletar-${sufixo}`);
  // A lista abre acima do campo e dentro da janela.
  const caixa = await pagina.locator("#mencoes-lista").evaluate((el) => {
    const r = el.getBoundingClientRect();
    const c = document.getElementById("campo-mensagem").getBoundingClientRect();
    return { acima: r.bottom <= c.top + 1, dentro: r.top >= 0 && r.left >= 0 && r.right <= innerWidth };
  });
  assert.deepEqual(caixa, { acima: true, dentro: true }, "a lista do @ cobre o campo ou sai da janela");
  await campo.press("ArrowDown");
  assert.equal(await campo.getAttribute("aria-activedescendant"), "mencao-1");
  await campo.press("ArrowUp");
  await campo.press("ArrowUp");
  assert.equal(await campo.getAttribute("aria-activedescendant"), "mencao-3", "a seta para cima não dá a volta");
  // O filtro.
  await campo.pressSequentially("ra");
  assert.deepEqual(await pagina.locator("#mencoes-lista li.mencoes-opcao .mencoes-nome").allTextContents(), ["@Rafa"]);
  await campo.press("Enter");
  assert.equal(await campo.inputValue(), "oi @Rafa ", "o Enter não inseriu o apelido");
  assert.equal(await visivel(pagina, "#mencoes-lista"), false);
  assert.equal(await campo.getAttribute("aria-activedescendant"), null);
  assert.equal((await chamadasDe(pagina, "send_message")).length, 2, "o Enter da lista enviou a mensagem");
  // O Tab escolhe, e o homônimo entra com a marca.
  await campo.pressSequentially("e @li");
  await campo.press("ArrowDown");
  await campo.press("Tab");
  assert.equal(await campo.inputValue(), "oi @Rafa e @Lia#Z9Z9 ");
  assert.equal(await pagina.evaluate(() => document.activeElement?.id), "campo-mensagem", "o Tab da lista tirou o foco do campo");
  // O Esc fecha a lista, e só ela: o texto fica, e o mesmo `@` não a reabre.
  await campo.pressSequentially("@ma");
  await pagina.waitForSelector("#mencoes-lista:not([hidden])");
  await campo.press("Escape");
  assert.equal(await visivel(pagina, "#mencoes-lista"), false);
  assert.equal(await campo.inputValue(), "oi @Rafa e @Lia#Z9Z9 @ma");
  await campo.pressSequentially("r");
  assert.equal(await visivel(pagina, "#mencoes-lista"), false, "a lista reabriu no @ que a pessoa dispensou");
  // Ninguém casa: a lista diz isso, e o Enter envia.
  await campo.fill("");
  await campo.pressSequentially("@zzz");
  assert.equal(await texto(pagina, "#mencoes-lista .mencoes-vazio"), "Ninguém neste servidor com esse nome.");
  // Um endereço não é menção (a regra 1 de `mencao.rs`).
  await campo.fill("");
  await campo.pressSequentially("escreva para x@ra");
  assert.equal(await visivel(pagina, "#mencoes-lista"), false, "o @ de um endereço abriu a lista");
  // O clique escolhe sem tirar o foco do campo.
  await campo.fill("");
  await campo.pressSequentially("@");
  await pagina.locator("#mencoes-lista li.mencoes-opcao", { hasText: "@Marina" }).click();
  assert.equal(await campo.inputValue(), "@Marina ");
  assert.equal(await pagina.evaluate(() => document.activeElement?.id), "campo-mensagem");
  await campo.fill("");

  // ---- 3. o aviso escrito e o som de quem é chamado
  await pagina.evaluate(() => {
    mesa.retrato.notice = { severity: "Info", reason: "Mentioned", operator_text: null };
    return atualizar();
  });
  // **A caixa de ALERTA não abre para uma menção**: ela é modal e oferece
  // MODERAR UMA PESSOA. O que diz que a pessoa foi chamada é o som, a mensagem
  // destacada e o «@ N» do canal. E um aviso de verdade continua abrindo-a.
  assert.equal(await visivel(pagina, "#banner"), false, "a menção abriu a caixa modal de alerta");
  await pagina.evaluate(() => {
    mesa.retrato.notice = { severity: "Warning", reason: "PermissionDenied", operator_text: null };
    return atualizar();
  });
  assert.equal(await visivel(pagina, "#banner"), true, "um aviso de verdade deixou de abrir a caixa");
  await pagina.evaluate(() => {
    mesa.retrato.notice = { severity: "Info", reason: "Mentioned", operator_text: null };
    return atualizar();
  });
  assert.equal(await visivel(pagina, "#banner"), false, "a menção não fechou a caixa que encontrou aberta");
  await captura(pagina, `aviso-de-chamada-${sufixo}`);
  const tocou = async () => {
    const antes = await pagina.evaluate(() => mesa.osciladores);
    await pagina.evaluate(() => mesa.emitir({ NoticeRaised: { notice: { severity: "Info", reason: "Mentioned", operator_text: null } } }));
    await pagina.waitForTimeout(120);
    return (await pagina.evaluate(() => mesa.osciladores)) - antes;
  };
  assert.ok(await tocou() >= 1, "ser chamado não tocou o som de mensagem");
  // Outro aviso não toca o som de mensagem.
  const antesDoOutro = await pagina.evaluate(() => mesa.osciladores);
  await pagina.evaluate(() => mesa.emitir({ NoticeRaised: { notice: { severity: "Warning", reason: "PermissionDenied", operator_text: null } } }));
  await pagina.waitForTimeout(120);
  assert.equal(await pagina.evaluate(() => mesa.osciladores), antesDoOutro, "um aviso que não é chamada tocou o som");
  // Com o grupo «mensagens» desligado, calado. (Mais de 1 s depois: o som tem intervalo mínimo.)
  await pagina.waitForTimeout(1100);
  await pagina.evaluate(() => alertasSonoros.definir({ grupos: { mensagens: false } }));
  assert.equal(await tocou(), 0, "o som tocou com o grupo de mensagens desligado");
  await pagina.evaluate(() => alertasSonoros.definir({ grupos: { mensagens: true } }));
  assert.ok(await tocou() >= 1, "o som não voltou com o grupo religado");
  await pagina.evaluate(() => { mesa.retrato.notice = null; return atualizar(); });

  // ---- 6. BANIDOS
  // A lista de pessoas é uma gaveta na janela estreita, e o Esc que fecha a
  // caixa de moderar fecha também a gaveta de trás: ela é reaberta a cada vez.
  const abrirBanidos = async () => {
    if (largura <= 1240) await pagina.evaluate(() => alternarPessoas(true));
    await pagina.locator("#abrir-banidos").click();
  };
  if (largura <= 1240) await pagina.evaluate(() => alternarPessoas(true));
  assert.equal(await visivel(pagina, "#abrir-banidos"), true, "quem pode banir não vê a porta dos banidos");
  await abrirBanidos();
  await pagina.waitForSelector("#moderar:not([hidden]) #moderar-banidos:not([hidden])");
  assert.equal(await texto(pagina, "#moderar-titulo"), "Banidos deste servidor");
  assert.equal((await chamadasDe(pagina, "listar_banidos")).length, 1, "abrir a lista não a pediu ao servidor");
  await pagina.waitForSelector("#moderar-banidos-lista > li");
  const fileiras = await pagina.locator("#moderar-banidos-lista > li").evaluateAll((els) => els.map((el) => ({
    nome: el.querySelector(".moderar-banido-nome").textContent,
    motivo: el.querySelector(".moderar-banido-motivo").textContent,
    prazo: el.querySelector(".moderar-banido-prazo").textContent,
    botao: el.querySelector("button[data-desbanir]").textContent,
    rotulo: el.querySelector("button[data-desbanir]").getAttribute("aria-label"),
  })));
  assert.equal(fileiras.length, 2);
  assert.deepEqual({ ...fileiras[0] }, { nome: "kaworu", motivo: "motivo: spam de convites", prazo: "permanente", botao: "DESBANIR", rotulo: "Desbanir kaworu" });
  assert.equal(fileiras[1].motivo, "sem motivo escrito");
  assert.match(fileiras[1].prazo, /^até \d/);
  assert.equal(await visivel(pagina, "#moderar-escolha"), false, "a escolha de pessoa aparece junto da lista de banidos");
  await captura(pagina, `banidos-${sufixo}`);
  // O DESBANIR arma a confirmação, com a consequência e o nome; nada saiu ainda.
  await pagina.locator('#moderar-banidos-lista button[data-desbanir="21"]').click();
  assert.equal(await visivel(pagina, "#moderar-confirmacao"), true);
  assert.equal(await texto(pagina, "#moderar-titulo"), "Desbanir kaworu?");
  assert.match(await texto(pagina, "#moderar-consequencia"), /^kaworu volta a poder entrar neste servidor, e reaparece entre as pessoas/);
  assert.equal(await texto(pagina, "#moderar-confirmar"), "DESBANIR kaworu");
  assert.equal(await pagina.evaluate(() => document.activeElement?.id), "moderar-cancelar", "o foco não pousou no CANCELAR");
  assert.deepEqual(await chamadasDe(pagina, "desbanir_pessoa"), [], "o DESBANIR saiu sem confirmação");
  await captura(pagina, `desbanir-confirmar-${sufixo}`);
  // CANCELAR volta à lista, e não fecha a caixa.
  await pagina.locator("#moderar-cancelar").click();
  assert.equal(await visivel(pagina, "#moderar-banidos"), true, "cancelar o desbanir não voltou à lista");
  assert.equal(await pagina.locator("#moderar-banidos-lista > li").count(), 2);
  // CONFIRMAR manda, volta à lista, e a pessoa sai dela.
  await pagina.locator('#moderar-banidos-lista button[data-desbanir="21"]').click();
  await pagina.locator("#moderar-confirmar").click();
  await pagina.waitForFunction(() => document.querySelectorAll("#moderar-banidos-lista > li").length === 1);
  assert.deepEqual(await chamadasDe(pagina, "desbanir_pessoa"), [{ person: 21 }]);
  assert.equal(await visivel(pagina, "#moderar"), true, "a caixa fechou depois do desbanir");
  // O último sai: o estado vazio tem frase.
  await pagina.locator('#moderar-banidos-lista button[data-desbanir="22"]').click();
  await pagina.locator("#moderar-confirmar").click();
  await pagina.waitForFunction(() => document.getElementById("moderar-banidos-estado").textContent === "Ninguém está banido deste servidor agora.");
  assert.equal(await visivel(pagina, "#moderar-banidos-estado"), true);
  await captura(pagina, `banidos-vazio-${sufixo}`);
  await pagina.keyboard.press("Escape");
  assert.equal(await visivel(pagina, "#moderar"), false);
  // Reabrir pede de novo, e enquanto a resposta não chega a caixa diz que lê.
  await pagina.evaluate(() => { mesa.listaMuda = true; });
  await abrirBanidos();
  await pagina.waitForSelector("#moderar-banidos:not([hidden])");
  assert.equal(await texto(pagina, "#moderar-banidos-estado"), "Lendo a lista de banidos…");
  assert.equal((await chamadasDe(pagina, "listar_banidos")).length, 2, "reabrir não pediu a lista de novo");
  // A recusa que o core dá na hora aparece com a frase dela.
  await pagina.keyboard.press("Escape");
  await pagina.evaluate(() => { mesa.falhar = { listar_banidos: "NotConnected" }; });
  await abrirBanidos();
  await pagina.waitForFunction(() => document.getElementById("moderar-banidos-estado").textContent === "SEM CONEXÃO");
  await pagina.keyboard.press("Escape");
  await pagina.evaluate(() => { mesa.falhar = null; mesa.listaMuda = false; });

  // A frase de BANIR diz onde o banimento se desfaz, num servidor que tem a lista.
  await pagina.evaluate(() => abrirModeracao("2"));
  await pagina.waitForSelector("#moderar:not([hidden]) #moderar-banir");
  await pagina.locator("#moderar-banir").click();
  await pagina.waitForSelector("#moderar-confirmacao:not([hidden])");
  assert.match(await texto(pagina, "#moderar-consequencia"), /até alguém que pode banir tirar o banimento: a lista fica em BANIDOS/);
  assert.doesNotMatch(await texto(pagina, "#moderar-consequencia"), /Nenhuma tela deste produto desfaz/);
  await pagina.keyboard.press("Escape");

  // Quem não pode banir não vê a porta.
  if (largura <= 1240) await pagina.evaluate(() => alternarPessoas(true));
  assert.equal(await visivel(pagina, "#abrir-banidos"), true);
  await pagina.evaluate(() => { mesa.retrato.may_ban = false; return atualizar(); });
  assert.equal(await visivel(pagina, "#abrir-banidos"), false, "quem não pode banir vê a porta dos banidos");

  assert.deepEqual(erros, [], `erros de página (${sufixo})`);
  await contexto.close();
}

// ------------------------------------------------- um servidor de antes da 1.0

async function noServidorAntigo(navegador, servidor, motor) {
  const { pagina, contexto, erros } = await abrir(navegador, servidor, { protocolo: 12 });
  const campo = pagina.locator("#campo-mensagem");
  assert.equal(await pagina.locator("#lista-mensagens button[data-editar]").count(), 0, "o EDITAR aparece num servidor 12");
  assert.equal(await pagina.locator("#lista-mensagens button[data-responder]").count(), 5, "o RESPONDER sumiu num servidor 12");
  await campo.press("ArrowUp");
  assert.equal(await visivel(pagina, "#compor-modo"), false, "a seta para cima editou num servidor 12");
  // E não diz «não há mensagem sua para editar»: ali a tecla não é da edição.
  await pagina.waitForTimeout(120);
  assert.doesNotMatch(await texto(pagina, "#anuncio"), /para editar/, "num servidor 12 a seta para cima fala de uma edição que não existe");
  if (await pagina.locator("#abrir-banidos").count()) {
    assert.equal(await visivel(pagina, "#abrir-banidos"), false, "a porta dos banidos aparece num servidor 12");
  }
  // Responder é do protocolo 9, e sai.
  await pagina.locator(`${mensagem(1)} button[data-responder]`).click();
  await campo.fill("respondo assim mesmo");
  await campo.press("Enter");
  await pagina.waitForFunction(() => mesa.chamadas.some((c) => c.cmd === "send_message"));
  assert.deepEqual(await chamadasDe(pagina, "send_message"), [{ channel: 1, body: "respondo assim mesmo", repliesTo: 1 }]);
  // E a frase de BANIR continua dizendo que ali não há volta.
  await pagina.evaluate(() => abrirModeracao("2"));
  await pagina.waitForSelector("#moderar:not([hidden]) #moderar-banir");
  await pagina.locator("#moderar-banir").click();
  await pagina.waitForSelector("#moderar-confirmacao:not([hidden])");
  assert.match(await texto(pagina, "#moderar-consequencia"), /Nenhuma tela deste produto desfaz um banimento neste servidor/);
  await pagina.keyboard.press("Escape");
  // O servidor é atualizado no meio da sessão (uma reconexão negocia o 13): o EDITAR aparece sem ninguém escrever.
  await pagina.evaluate(() => { mesa.protocolo = 13; mesa.retrato.protocolo = 13; return atualizar(); });
  await pagina.waitForFunction(() => document.querySelectorAll("#lista-mensagens button[data-editar]").length === 3);
  assert.deepEqual(erros, [], `erros de página (${motor}, servidor 12)`);
  await contexto.close();
}

// --------------------------------------------------------------- o celular

async function noCelular(navegador, servidor) {
  const { pagina, contexto, erros } = await abrir(navegador, servidor, { celular: true, largura: 390, altura: 844 });
  assert.equal(await pagina.evaluate(() => document.documentElement.dataset.casca), "celular");
  const campo = pagina.locator("#campo-mensagem");

  // A citação e o destaque valem aqui; os botões da fileira, não: são da folha.
  assert.equal(await texto(pagina, `${mensagem(3)} .mensagem-citacao-autor`), "Alex");
  assert.equal(await visivel(pagina, `${mensagem(3)} .mensagem-citacao`), true);
  assert.equal(await texto(pagina, `${mensagem(4)} .mensagem-citacao-ausente`), "mensagem anterior");
  assert.equal(await texto(pagina, `${mensagem(3)} .mensagem-chamou-marca`), "chamou você");
  assert.equal(await visivel(pagina, `${mensagem(3)} .mensagem-chamou-marca`), true);
  assert.equal(await visivel(pagina, `${mensagem(1)} button[data-responder]`), false, "o RESPONDER da fileira aparece no celular");
  await captura(pagina, "resposta-e-mencao-celular-390x844");

  // A folha da mensagem de outra pessoa: RESPONDER, sem EDITAR.
  await pagina.locator(`${mensagem(1)} .corpo`).click();
  await pagina.waitForSelector("#cel-folha-mensagem");
  const acoesDaOutra = await pagina.locator("#cel-folha-mensagem .cel-folha-acoes button").evaluateAll((bs) => bs.map((b) => b.dataset.acao));
  assert.deepEqual(acoesDaOutra, ["responder", "copiar"]);
  await pagina.locator('#cel-folha-mensagem button[data-acao="responder"]').click();
  assert.equal(await texto(pagina, "#compor-modo-frase"), "Respondendo a Lia");
  assert.equal(await visivel(pagina, "#compor-modo"), true, "a faixa de resposta não aparece no celular");
  const faixa = await pagina.locator("#compor-modo").evaluate((el) => {
    const r = el.getBoundingClientRect();
    const c = document.getElementById("form-mensagem").getBoundingClientRect();
    return { dentro: r.left >= 0 && r.right <= innerWidth + 0.5, acima: r.bottom <= c.top + 1 };
  });
  assert.deepEqual(faixa, { dentro: true, acima: true }, "a faixa do modo sai da tela ou cobre o campo");
  await captura(pagina, "respondendo-celular-390x844");
  await campo.fill("vou por aí");
  // No celular o Enter quebra a fileira: quem envia é o botão.
  await pagina.locator("#botao-enviar").click();
  await pagina.waitForFunction(() => mesa.chamadas.some((c) => c.cmd === "send_message"));
  assert.deepEqual(await chamadasDe(pagina, "send_message"), [{ channel: 1, body: "vou por aí", repliesTo: 1 }]);

  // A folha da própria: RESPONDER, COPIAR, EDITAR e APAGAR.
  await pagina.locator(`${mensagem(5)} .corpo`).click();
  await pagina.waitForSelector("#cel-folha-mensagem");
  const acoesDaMinha = await pagina.locator("#cel-folha-mensagem .cel-folha-acoes button").evaluateAll((bs) => bs.map((b) => b.dataset.acao));
  assert.deepEqual(acoesDaMinha, ["responder", "copiar", "editar", "apagar"]);
  await captura(pagina, "folha-da-mensagem-celular-390x844");
  await pagina.locator('#cel-folha-mensagem button[data-acao="editar"]').click();
  assert.equal(await pagina.locator("#compor-modo").getAttribute("data-modo"), "editar");
  assert.equal(await campo.inputValue(), "Levo. E as pilhas.");
  assert.equal(await pagina.locator("#botao-enviar").getAttribute("aria-label"), "Salvar a edição");
  await captura(pagina, "editando-celular-390x844");
  await campo.fill("Levo. E as pilhas novas.");
  await pagina.locator("#botao-enviar").click();
  await pagina.waitForSelector(`${mensagem(5)} .editada`);
  assert.deepEqual(await chamadasDe(pagina, "editar_mensagem"), [{ message: 5, body: "Levo. E as pilhas novas." }]);
  assert.equal(await pagina.locator("#botao-enviar").getAttribute("aria-label"), "Enviar a mensagem");
  // A saída da edição sem teclado físico: o botão da faixa.
  await pagina.locator(`${mensagem(5)} .corpo`).click();
  await pagina.locator('#cel-folha-mensagem button[data-acao="editar"]').click();
  await pagina.locator("#compor-modo-cancelar").click();
  assert.equal(await visivel(pagina, "#compor-modo"), false);
  const alvoDoCancelar = await pagina.evaluate(() => {
    document.querySelector('#lista-mensagens > li[data-mensagem-id="5"] button[data-editar]').click();
    const r = document.getElementById("compor-modo-cancelar").getBoundingClientRect();
    return { largura: r.width, altura: r.height };
  });
  assert.ok(alvoDoCancelar.altura >= 44 && alvoDoCancelar.largura >= 44, `o cancelar da faixa é pequeno para o dedo: ${JSON.stringify(alvoDoCancelar)}`);
  await pagina.locator("#compor-modo-cancelar").click();

  // O `@` no celular: a lista abre acima do campo, e o toque escolhe.
  await campo.fill("");
  await campo.pressSequentially("@l");
  await pagina.waitForSelector("#mencoes-lista:not([hidden]) li.mencoes-opcao");
  const lista = await pagina.locator("#mencoes-lista").evaluate((el) => {
    const r = el.getBoundingClientRect();
    const c = document.getElementById("campo-mensagem").getBoundingClientRect();
    const alvos = [...el.querySelectorAll("li.mencoes-opcao")].map((li) => li.getBoundingClientRect().height);
    return { dentro: r.left >= 0 && r.right <= innerWidth + 0.5 && r.top >= 0, acima: r.bottom <= c.top + 1, menorAlvo: Math.min(...alvos) };
  });
  assert.equal(lista.dentro && lista.acima, true, `a lista do @ sai da tela ou cobre o campo: ${JSON.stringify(lista)}`);
  assert.ok(lista.menorAlvo >= 44, `a opção do @ é pequena para o dedo (${lista.menorAlvo}px)`);
  await captura(pagina, "autocompletar-celular-390x844");
  await pagina.locator("#mencoes-lista li.mencoes-opcao").first().click();
  assert.equal(await campo.inputValue(), "@Lia#K7Q2 ");
  await campo.fill("");

  // As não lidas, na aba SERVIDOR; e a conta some ao abrir o canal.
  await pagina.evaluate(() => { document.getElementById("tela-sessao").dataset.aba = "servidor"; });
  await pagina.waitForTimeout(150);
  const canal = (id) => `#lista-linhas button[data-linha="${id}"]`;
  assert.equal(await visivel(pagina, `${canal(3)} .linha-nao-lidas-conta`), true, "a conta não aparece na aba SERVIDOR");
  assert.equal(await texto(pagina, `${canal(3)} .linha-nao-lidas-conta`), "@ 5");
  assert.equal(await texto(pagina, `${canal(2)} .linha-nao-lidas-conta`), "3");
  const fileira = await pagina.locator(canal(3)).evaluate((el) => {
    const r = el.getBoundingClientRect();
    const conta = el.querySelector(".linha-nao-lidas").getBoundingClientRect();
    return { dentro: conta.right <= r.right + 0.5 && conta.left >= r.left, naTela: r.right <= innerWidth + 0.5 };
  });
  assert.deepEqual(fileira, { dentro: true, naTela: true }, "a conta sai da fileira do canal");
  await captura(pagina, "canais-nao-lidas-celular-390x844");
  await pagina.locator(canal(3)).click();
  await pagina.waitForFunction(() => document.querySelector('#lista-linhas button[data-linha="3"]').classList.contains("aberto"));
  assert.equal(await pagina.locator(`${canal(3)} .linha-nao-lidas`).count(), 0);

  // Moderar é pelo computador: a porta dos banidos não aparece no celular.
  await pagina.evaluate(() => { document.getElementById("tela-sessao").dataset.aba = "pessoas"; });
  await pagina.waitForTimeout(150);
  assert.equal(await visivel(pagina, "#abrir-banidos"), false, "a porta dos banidos aparece no celular");

  assert.deepEqual(erros, [], "erros de página (celular)");
  await contexto.close();
}

// O dublê e a abertura servem também a quem só quer olhar a tela (as capturas
// de revisão moram fora do repositório e pedem estes dois).
module.exports = { abrir, estadoInicial, instalarAPonte };

if (require.main === module) (async () => {
  const servidor = servir();
  await new Promise((ok) => servidor.listen(0, "127.0.0.1", ok));
  try {
    for (const motor of ["chromium", "webkit"]) {
      const navegador = await motores[motor]().launch({
        headless: true,
        ...(motor === "chromium" ? { args: ["--autoplay-policy=no-user-gesture-required"] } : {}),
      });
      try {
        for (const medida of [[1280, 800], [720, 480]]) await noComputador(navegador, servidor, motor, medida);
        await noServidorAntigo(navegador, servidor, motor);
        if (motor === "chromium") await noCelular(navegador, servidor);
        console.log(`${motor}: editar, responder, menção, @, não lidas e banidos OK em 1280×800 e 720×480; servidor 12 sem EDITAR nem BANIDOS${motor === "chromium" ? "; celular 390×844 pela folha da mensagem" : ""}`);
      } finally {
        await navegador.close();
      }
    }
  } finally {
    servidor.close();
  }
})().catch(async (erro) => {
  console.error(erro);
  // O que estava na tela quando reprovou, para quem vai consertar.
  if (process.env.SEELE_PREVIEW_DIR && paginaEmCurso) {
    await paginaEmCurso.screenshot({ path: path.join(process.env.SEELE_PREVIEW_DIR, "reprovou.png") }).catch(() => {});
  }
  process.exit(1);
});
