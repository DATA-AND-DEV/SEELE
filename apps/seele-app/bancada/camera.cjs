// **A câmera, na tela e no codificador** (1.0, protocolo 13).
//
// A UI é a real (`ui/` como está, com a CSP do produto) e a câmera é a de
// mentira do navegador (`--use-fake-device-for-media-stream`): o `getUserMedia`
// entrega quadros de verdade, e o `VideoEncoder` os codifica de verdade. A
// ponte é um dublê **do aplicativo**, com estado: ela responde como o núcleo
// responde — o `compartilhar_camera` dá nome à transmissão e a abre com um
// `ScreenOpened` que diz câmera, o `quadro_da_camera` recebe o corpo cru com
// os cabeçalhos, e o servidor do protocolo 12 volta `ServidorAntigo`.
//
// # O que ela prende
//
//  1. O controle CÂMERA só existe numa sala, ao lado do COMPARTILHAR, com o
//     nome em palavras e o estado no botão.
//  2. Ligar pede a câmera, mostra a prévia da própria mídia (e não quadros
//     devolvidos pela ponte), e manda quadros H.264 em Annex-B pelo
//     `quadro_da_camera`, em corpo cru e com os três cabeçalhos: **o primeiro é
//     chave**, traz SPS/PPS, e as medidas são as que o núcleo mandou.
//  3. Um `CameraKeyFrameRequested` faz o próximo quadro sair como chave; um
//     `CameraBudget` reconfigura o codificador; um `CameraPaused` para de
//     mandar e diz por quê, e o `ScreenOpened` seguinte retoma com chave.
//  4. A fila cheia é dita na tela com a contagem, e o quadro seguinte é chave.
//  5. Com a tela no ar, ligar a câmera avisa que ela entra no lugar da tela.
//  6. Negada pelo sistema, sem câmera nenhuma, sem codificador nesta janela:
//     cada uma com a frase dela, e a câmera fica desligada.
//  7. Num servidor do protocolo 12 o controle fica apagado, com o porquê, e o
//     clique não pede a câmera ao sistema nem chama o núcleo.
//  8. Desligar solta a câmera e avisa o servidor; o servidor que encerra a
//     transmissão apaga a câmera e diz.
//  9. Quem assiste lê «CÂMERA» no palco quando a abertura diz câmera, «TELA»
//     quando diz tela, e nenhum dos dois antes de ela chegar.
// 10. Nada disso estoura a página em 1280×800 nem em 720×480.
// 11. Na página do navegador, a mídia da câmera entra no transmissor da tela
//     (`compartilhar-web.js`): o pedido é o da câmera, a abertura declara a
//     câmera, o primeiro quadro é chave, e os quadros dela não voltam à casca.
//
//   PLAYWRIGHT=<playwright-core> node apps/seele-app/bancada/camera.cjs
//
// Com `SEELE_PREVIEW_DIR`, grava as capturas de cada estado. Com
// `SEELE_CAMERA_WEBKIT=1`, roda também no WebKit o que ele consegue rodar (os
// estados sem câmera: o WebKit do Playwright não tem câmera de mentira).

const assert = require("node:assert/strict");
const path = require("node:path");
const { servir, respostas, retrato } = require("./telas.cjs");
const motores = require("./playwright.cjs");

const pessoa = (id, nickname, extra = {}) => ({
  id, nickname, speaking: false, muted: false, total_isolation: false, signal: 90, sync_band: "Nominal", is_self: false, ...extra,
});

function estadoInicial({ protocolo, telaNoAr }) {
  const minha = telaNoAr ? { tela: 3, de: 1, e_minha: true, espectadores: 1, parada: null, pedido: null } : null;
  return {
    protocolo,
    proximaTela: 7,
    retrato: retrato({
      protocolo,
      me: 1,
      voice_rooms: [{
        id: 1, name: "Sala principal", limit: 8, password_required: false, occupied_by_us: true, channel: 1,
        people: [pessoa(1, "Alex", { is_self: true }), pessoa(2, "Lia")], sync: null,
      }],
      presentes: [pessoa(1, "Alex", { is_self: true }), pessoa(2, "Lia")],
      tela: minha,
      minha_transmissao: minha,
      transmissoes: minha ? [{ tela: 3, de: 1, e_minha: true, assistida: false, espectadores: 1 }] : [],
    }),
  };
}

/** A ponte de mentira do aplicativo, dentro da página. O estado está em `window.banca`. */
function instalarAPonte({ estado, tabela, semCodificador, semCamera }) {
  const ouvintes = new Map();
  const banca = { ...estado, chamadas: [], quadros: [], falharOQuadro: null, pedidosDeMidia: 0, mudo: false };
  window.banca = banca;
  const emitir = (payload) => {
    for (const fn of ouvintes.get("seele://event") ?? []) fn({ payload });
  };
  banca.emitir = emitir;
  if (semCodificador) delete window.VideoEncoder;
  if (semCamera === "sem-captura") Object.defineProperty(navigator, "mediaDevices", { value: undefined, configurable: true });
  else if (navigator.mediaDevices?.getUserMedia) {
    const original = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
    navigator.mediaDevices.getUserMedia = (pedido) => {
      banca.pedidosDeMidia += 1;
      banca.ultimoPedido = JSON.parse(JSON.stringify(pedido));
      if (semCamera) return Promise.reject(new DOMException("a banca recusou", semCamera));
      return original(pedido);
    };
  }
  const minha = (tela, pedido) => ({ tela, de: 1, e_minha: true, espectadores: 1, parada: null, pedido });
  const comandos = {
    snapshot: () => structuredClone(banca.retrato),
    // O que o núcleo responde: 720p a 30 no máximo, na proporção da câmera.
    codificacao_da_camera: ({ largura, altura }) => {
      const fator = Math.min(1, 720 / (altura || 720), 1280 / (largura || 1280));
      const par = (v) => Math.max(2, Math.floor((v * fator) / 2) * 2);
      return { codec: "avc1.42E028", largura: par(largura || 1280), altura: par(altura || 720), quadros: 30, banda_bps: 1_200_000 };
    },
    compartilhar_camera: ({ largura, altura, limites }) => {
      if (banca.protocolo < 13) throw { ServidorAntigo: { negociado: banca.protocolo, exige: 13 } };
      const tela = banca.proximaTela++;
      banca.camera = { tela, largura, altura };
      banca.retrato.minha_transmissao = minha(tela, limites);
      banca.retrato.tela = minha(tela, limites);
      banca.retrato.transmissoes = [{ tela, de: 1, e_minha: true, assistida: false, espectadores: 1 }];
      emitir("ScreenChanged");
      if (banca.mudo) return null;
      setTimeout(() => {
        emitir({ CameraBudget: { screen: tela, bps: 1_200_000 } });
        emitir({ ScreenOpened: { screen: tela, width: largura, height: altura, source: "camera" } });
      }, 60);
      return null;
    },
    parar_de_compartilhar: () => {
      const tela = banca.retrato.minha_transmissao?.tela ?? null;
      banca.camera = null;
      banca.retrato.minha_transmissao = null;
      banca.retrato.tela = null;
      banca.retrato.transmissoes = [];
      emitir("ScreenChanged");
      if (tela !== null) setTimeout(() => emitir({ ScreenClosed: { screen: tela } }), 20);
      return null;
    },
  };
  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args, opcoes) => {
        if (cmd === "quadro_da_camera") {
          // O corpo cru e os cabeçalhos, como o Tauri os entrega ao comando.
          const bytes = args instanceof Uint8Array ? args : null;
          const quadro = {
            cru: bytes !== null,
            tamanho: bytes?.length ?? -1,
            inicio: bytes ? Array.from(bytes.subarray(0, 5)) : [],
            nals: [],
            cabecalhos: { ...(opcoes?.headers ?? {}) },
            quando: performance.now(),
          };
          // Os tipos das NAL do quadro, pelos códigos de início do Annex-B.
          for (let i = 0; bytes && i + 4 < bytes.length; i++) {
            if (bytes[i] === 0 && bytes[i + 1] === 0 && (bytes[i + 2] === 1 || (bytes[i + 2] === 0 && bytes[i + 3] === 1))) {
              const onde = bytes[i + 2] === 1 ? i + 3 : i + 4;
              quadro.nals.push(bytes[onde] & 0x1f);
              i = onde;
            }
          }
          banca.quadros.push(quadro);
          if (banca.falharOQuadro) {
            const falha = typeof banca.falharOQuadro === "function" ? banca.falharOQuadro() : banca.falharOQuadro;
            if (falha) throw falha;
          }
          return null;
        }
        banca.chamadas.push({ cmd, args: args ? JSON.parse(JSON.stringify(args)) : null });
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
}

let paginaEmCurso = null;

async function abrir(navegador, servidor, { protocolo = 13, telaNoAr = false, largura = 1280, altura = 800, semCodificador = false, semCamera = null } = {}) {
  const contexto = await navegador.newContext({ viewport: { width: largura, height: altura }, deviceScaleFactor: 1 });
  const pagina = await contexto.newPage();
  pagina.setDefaultTimeout(10_000);
  const erros = [];
  pagina.on("pageerror", (e) => erros.push(e.message));
  pagina.on("console", (m) => { if (process.env.SEELE_CAMERA_FALA && m.type() !== "debug") console.log(`    [página] ${m.text()}`); });
  await pagina.addInitScript(instalarOsOlhos);
  await pagina.addInitScript(instalarAPonte, {
    estado: estadoInicial({ protocolo, telaNoAr }), tabela: respostas({ snapshot: null }), semCodificador, semCamera,
  });
  await pagina.goto(`http://127.0.0.1:${servidor.address().port}/`, { waitUntil: "domcontentloaded" });
  await pagina.waitForTimeout(700);
  await pagina.evaluate(async () => {
    for (const s of document.querySelectorAll("section.tela")) s.hidden = true;
    document.getElementById("tela-sessao").hidden = false;
    await atualizar();
    await abrirChamada();
  });
  await pagina.waitForFunction(() => Boolean(olhos.controle()));
  paginaEmCurso = pagina;
  return { pagina, contexto, erros };
}

/**
 * Dentro da página: o controle da câmera e a frase dela **que estão à vista**.
 * Na vista da chamada a coluna do operador sai de cena, e quem aparece é o
 * espelho do botão na bandeja de baixo, com a frase na própria bandeja; fora
 * dela, o botão e a frase do cartão do operador.
 */
function instalarOsOlhos() {
  const aVista = (el) => { if (!el) return false; const r = el.getBoundingClientRect(); return !el.closest("[hidden]") && r.width > 0 && r.height > 0; };
  window.olhos = {
    aVista,
    controle: () => [document.getElementById("operador-camera"), ...document.querySelectorAll("#chamada-controles button")]
      .filter((b) => b && /CÂMERA/.test(b.getAttribute("aria-label") ?? "")).find(aVista) ?? null,
    compartilhar: () => [document.getElementById("operador-vista"), ...document.querySelectorAll("#chamada-controles button")]
      .filter((b) => b && /COMPARTILHAR/.test(b.getAttribute("aria-label") ?? "")).find(aVista) ?? null,
    frase: () => [document.getElementById("camera-estado"), document.getElementById("chamada-camera-estado")].find(aVista) ?? null,
  };
}

const chamadasDe = (pagina, cmd) => pagina.evaluate((cmd) => banca.chamadas.filter((c) => c.cmd === cmd).map((c) => c.args), cmd);
const botao = (pagina) => pagina.evaluate(() => {
  const b = olhos.controle();
  if (!b) return { escondido: true };
  const r = b.getBoundingClientRect();
  return {
    escondido: false, rotulo: b.getAttribute("aria-label"), apertado: b.getAttribute("aria-pressed"),
    desligado: b.getAttribute("aria-disabled"), estado: document.getElementById("operador-camera").dataset.estado,
    dica: b.title, temDesenho: Boolean(b.querySelector("svg")),
    largura: Math.round(r.width), altura: Math.round(r.height),
  };
});
const estado = (pagina) => pagina.evaluate(() => {
  const p = olhos.frase();
  return p ? { escondido: false, texto: p.textContent, estado: p.dataset.estado } : { escondido: true, texto: "", estado: null };
});
const previa = (pagina) => pagina.evaluate(() => {
  const v = document.getElementById("camera-previa");
  const r = v.getBoundingClientRect();
  return {
    escondida: v.hidden || r.width === 0, temMidia: v.srcObject instanceof MediaStream,
    largura: v.videoWidth, altura: v.videoHeight, tocando: !v.paused, mudo: v.muted,
    trilhasVivas: v.srcObject ? v.srcObject.getVideoTracks().filter((t) => t.readyState === "live").length : 0,
  };
});
const semEstouro = (pagina) => pagina.evaluate(() => ({
  largura: document.documentElement.scrollWidth <= window.innerWidth + 1,
  altura: document.documentElement.scrollHeight <= window.innerHeight + 1,
}));

async function captura(pagina, nome) {
  const pasta = process.env.SEELE_PREVIEW_DIR;
  if (!pasta) return;
  await pagina.waitForTimeout(200);
  await pagina.screenshot({ path: path.join(pasta, `${nome}.png`) });
}

const clicar = async (pagina) => {
  const alvo = await pagina.evaluateHandle(() => olhos.controle());
  assert.ok(alvo.asElement(), "o controle da câmera não está à vista");
  // O controle apagado (`aria-disabled`) continua clicável para uma pessoa — é
  // o clique que escreve o porquê —, e o Playwright o trata como desabilitado:
  // `force` é o dedo de quem clica assim mesmo.
  await alvo.asElement().click({ force: true });
};
const quadros = (pagina) => pagina.evaluate(() => banca.quadros.map((q) => ({ ...q })));

// --------------------------------------------------------------- os casos

/** 1 a 4, 8 e 10: a câmera de mentira, o codificador de verdade. */
async function aCameraNoAr(navegador, servidor, medida) {
  const [largura, altura] = medida;
  const sufixo = `chromium-${largura}x${altura}`;
  const { pagina, contexto, erros } = await abrir(navegador, servidor, { largura, altura });

  // ---- 1. o controle, antes
  let b = await botao(pagina);
  assert.deepEqual(
    [b.escondido, b.rotulo, b.apertado, b.desligado, b.estado, b.temDesenho],
    [false, "LIGAR A CÂMERA", "false", "false", "desligada", true], `o controle antes de ligar: ${JSON.stringify(b)}`);
  assert.match(b.dica, /mostrar a sua câmera para quem está em Sala principal/);
  assert.ok(b.altura >= 40 && b.largura >= 40, `o alvo do controle é menor que o dos vizinhos: ${b.largura}×${b.altura}`);
  const vizinhos = () => pagina.evaluate(() => {
    const a = olhos.compartilhar().getBoundingClientRect();
    const c = olhos.controle().getBoundingClientRect();
    return { mesmaLinha: Math.abs(a.top - c.top) < 2, aoLado: c.left >= a.right - 1 && c.left - a.right < 40, mesmaAltura: Math.abs(a.height - c.height) < 2 };
  });
  assert.deepEqual(await vizinhos(), { mesmaLinha: true, aoLado: true, mesmaAltura: true }, "na chamada, a CÂMERA não está ao lado do COMPARTILHAR");
  // E no cartão do operador, com a conversa à frente: o mesmo controle, ao lado do mesmo vizinho.
  if (largura >= 900) {
    await pagina.evaluate(() => fecharChamada());
    await pagina.waitForFunction(() => olhos.controle()?.id === "operador-camera");
    await captura(pagina, `camera-antes-no-operador-${sufixo}`);
    assert.deepEqual(await vizinhos(), { mesmaLinha: true, aoLado: true, mesmaAltura: true }, "no cartão do operador, a CÂMERA não está ao lado do COMPARTILHAR");
    await pagina.evaluate(() => abrirChamada());
    await pagina.waitForFunction(() => olhos.controle() && olhos.controle().id !== "operador-camera");
  }
  assert.equal((await estado(pagina)).escondido, true, "o estado da câmera ocupa lugar antes de haver o que dizer");
  assert.equal((await previa(pagina)).escondida, true);
  await captura(pagina, `camera-antes-${sufixo}`);

  // ---- 2. ligar
  await clicar(pagina);
  await pagina.waitForFunction(() => document.getElementById("operador-camera").dataset.estado === "no-ar");
  b = await botao(pagina);
  assert.deepEqual([b.rotulo, b.apertado, b.estado], ["DESLIGAR A CÂMERA", "true", "no-ar"]);
  assert.equal(b.dica, "parar de mostrar a sua câmera");
  assert.match((await estado(pagina)).texto, /^SUA CÂMERA ESTÁ INDO PARA A SALA\./);
  assert.equal(await pagina.evaluate(() => banca.pedidosDeMidia), 1);
  const pedido = await pagina.evaluate(() => banca.ultimoPedido);
  assert.equal(pedido.audio, false, "a câmera pediu som");
  assert.ok(pedido.video.height.ideal <= 720 && pedido.video.frameRate.max <= 30, `a câmera pediu mais que 720p a 30: ${JSON.stringify(pedido)}`);

  // A ordem: as medidas ao núcleo, e só depois o pedido — com as medidas que ele deu.
  const [medidas] = await chamadasDe(pagina, "codificacao_da_camera");
  const [pedida] = await chamadasDe(pagina, "compartilhar_camera");
  assert.ok(medidas.largura > 0 && medidas.altura > 0, `a câmera não disse as medidas dela: ${JSON.stringify(medidas)}`);
  assert.ok(pedida.altura <= 720 && pedida.largura % 2 === 0 && pedida.altura % 2 === 0, `as medidas pedidas: ${JSON.stringify(pedida)}`);
  assert.deepEqual(pedida.limites, { banda_bps: null, altura_maxima: 720, quadros_maximos: 30, prioridade: "movimento", com_som: false });
  assert.ok(pedida.carimbo === undefined || typeof pedida.carimbo === "object");

  // A prévia: a própria mídia, viva, sem som, e com imagem.
  await pagina.waitForFunction(() => document.getElementById("camera-previa").videoWidth > 0);
  const p = await previa(pagina);
  assert.deepEqual([p.escondida, p.temMidia, p.mudo, p.trilhasVivas], [false, true, true, 1], `a prévia: ${JSON.stringify(p)}`);
  assert.equal(await pagina.evaluate(() => document.getElementById("camera-previa").srcObject.getVideoTracks()[0].contentHint), "motion");
  assert.equal(await pagina.evaluate(() => document.getElementById("palco-quem").textContent), "SUA CÂMERA");
  assert.equal(await pagina.evaluate(() => document.getElementById("palco-titulo-tipo").textContent), "CÂMERA");
  // O COMPARTILHAR não se acende pela câmera: ele continua oferecendo a tela.
  assert.equal(await pagina.evaluate(() => document.getElementById("operador-vista").dataset.modo), "compartilhar");
  assert.equal(await pagina.evaluate(() => document.getElementById("operador-trocar-tela").hidden), true);

  // Os quadros: corpo cru, os três cabeçalhos, e o primeiro é chave com SPS e PPS.
  await pagina.waitForFunction(() => banca.quadros.length >= 12, null, { timeout: 15_000 });
  let q = await quadros(pagina);
  assert.ok(q.every((quadro) => quadro.cru && quadro.tamanho > 0), "um quadro não foi em corpo cru, ou foi vazio");
  assert.deepEqual(Object.keys(q[0].cabecalhos).sort(), ["seele-chave", "seele-geracao", "seele-sessao"].slice(0, Object.keys(q[0].cabecalhos).length).sort());
  assert.equal(q[0].cabecalhos["seele-chave"], "1", "o primeiro quadro não saiu marcado como chave");
  assert.deepEqual(q[0].inicio.slice(0, 3), [0, 0, 0], `o primeiro quadro não começa por um código de início do Annex-B: ${q[0].inicio}`);
  assert.ok(q[0].nals.includes(7) && q[0].nals.includes(8) && q[0].nals.includes(5),
    `o quadro-chave não traz SPS, PPS e IDR em Annex-B: NALs ${q[0].nals}`);
  assert.ok(q.slice(1, 8).some((quadro) => quadro.cabecalhos["seele-chave"] === "0" && quadro.nals.includes(1)),
    "depois da chave não saiu nenhum quadro comum");
  assert.ok(q.filter((quadro) => quadro.cabecalhos["seele-chave"] === "1").length <= 2, "saíram quadros-chave sem ninguém pedir");
  // Depois da chave, a captura espera as fatias: o segundo quadro não sai colado nela.
  assert.ok(q[1].quando - q[0].quando >= 90, `o quadro seguinte à chave saiu ${Math.round(q[1].quando - q[0].quando)} ms depois: no meio das fatias dela`);
  await captura(pagina, `camera-no-ar-${sufixo}`);
  assert.deepEqual(await semEstouro(pagina), { largura: true, altura: true }, `a câmera no ar estoura a janela de ${largura}×${altura}`);

  // ---- 3. o pedido de chave, a banda e a parada
  const antes = q.length;
  await pagina.evaluate(() => banca.emitir({ CameraKeyFrameRequested: { screen: banca.camera.tela } }));
  await pagina.waitForFunction((antes) => banca.quadros.slice(antes).some((quadro) => quadro.cabecalhos["seele-chave"] === "1"), antes);
  q = await quadros(pagina);
  const pedidaAgora = q.slice(antes).find((quadro) => quadro.cabecalhos["seele-chave"] === "1");
  assert.ok(pedidaAgora.nals.includes(5), "o quadro pedido saiu marcado como chave sem ser um IDR");
  // De outra transmissão, o pedido não é desta câmera.
  const depoisDoPedido = (await quadros(pagina)).length;
  await pagina.evaluate(() => banca.emitir({ CameraKeyFrameRequested: { screen: 9999 } }));
  await pagina.waitForFunction((n) => banca.quadros.length >= n + 6, depoisDoPedido);
  assert.equal((await quadros(pagina)).slice(depoisDoPedido, depoisDoPedido + 6).filter((quadro) => quadro.cabecalhos["seele-chave"] === "1").length, 0,
    "o pedido de chave de outra transmissão fez esta câmera mandar uma");

  const configuracoes = await pagina.evaluate(async () => {
    const vistas = [];
    const original = VideoEncoder.prototype.configure;
    VideoEncoder.prototype.configure = function (c) { vistas.push(c.bitrate); return original.call(this, c); };
    banca.emitir({ CameraBudget: { screen: banca.camera.tela, bps: 600_000 } });
    await new Promise((ok) => setTimeout(ok, 150));
    VideoEncoder.prototype.configure = original;
    return vistas;
  });
  assert.equal(configuracoes.length, 1, "a banda nova não reconfigurou o codificador");
  assert.ok(configuracoes[0] < 600_000 && configuracoes[0] >= 400_000, `o codificador não mira abaixo do teto do fluxo: ${configuracoes[0]}`);

  await pagina.evaluate(() => banca.emitir({ CameraPaused: { screen: banca.camera.tela, reason: "SinalCritico" } }));
  await pagina.waitForFunction(() => document.getElementById("operador-camera").dataset.estado === "parada");
  assert.match((await estado(pagina)).texto, /^A CÂMERA PAROU PARA A VOZ NÃO PICOTAR\./);
  await pagina.waitForTimeout(250);
  const naParada = (await quadros(pagina)).length;
  await pagina.waitForTimeout(400);
  assert.ok((await quadros(pagina)).length - naParada <= 1, "a câmera parada pelo sinal continuou mandando quadros");
  assert.equal((await previa(pagina)).trilhasVivas, 1, "a parada pelo sinal soltou a câmera: ela não voltaria sozinha");
  await captura(pagina, `camera-parada-${sufixo}`);
  // A volta: o fluxo reabre, e o primeiro quadro é chave.
  const naVolta = await pagina.evaluate(() => {
    const agora = performance.now();
    banca.emitir({ ScreenOpened: { screen: banca.camera.tela, width: banca.camera.largura, height: banca.camera.altura, source: "camera" } });
    return agora;
  });
  await pagina.waitForFunction((quando) => banca.quadros.filter((quadro) => quadro.quando > quando).length > 3, naVolta);
  q = await quadros(pagina);
  const daVolta = q.filter((quadro) => quadro.quando > naVolta);
  assert.equal(daVolta[0].cabecalhos["seele-chave"], "1", "depois da parada o primeiro quadro não é chave");
  assert.ok(daVolta[0].nals.includes(7) && daVolta[0].nals.includes(5), `o primeiro quadro da volta não traz SPS e IDR: ${daVolta[0].nals}`);
  assert.equal((await botao(pagina)).estado, "no-ar");

  // ---- 4. a fila cheia: dita, contada, e o quadro seguinte é chave
  const antesDaFila = (await quadros(pagina)).length;
  await pagina.evaluate(() => {
    let vezes = 0;
    banca.falharOQuadro = () => (++vezes === 1 ? { CameraQueueFull: { dropped: 3 } } : null);
  });
  await pagina.waitForFunction(() => /QUADROS FICARAM/i.test(olhos.frase()?.textContent ?? ""));
  assert.match((await estado(pagina)).texto, /^A CONEXÃO NÃO ESTÁ LEVANDO A CÂMERA NO RITMO DELA\.\n3 quadros ficaram para trás/);
  assert.equal((await botao(pagina)).estado, "no-ar", "a fila cheia desligou a câmera");
  await pagina.waitForFunction((n) => banca.quadros.slice(n + 1).some((quadro) => quadro.cabecalhos["seele-chave"] === "1"), antesDaFila);
  await captura(pagina, `camera-fila-cheia-${sufixo}`);
  await pagina.evaluate(() => { banca.falharOQuadro = null; });

  // ---- 8. desligar
  await clicar(pagina);
  await pagina.waitForFunction(() => document.getElementById("operador-camera").dataset.estado === "desligada");
  assert.equal((await chamadasDe(pagina, "parar_de_compartilhar")).length, 1);
  const depois = await previa(pagina);
  assert.deepEqual([depois.escondida, depois.temMidia], [true, false], "a prévia ficou depois de desligar");
  b = await botao(pagina);
  assert.deepEqual([b.rotulo, b.apertado], ["LIGAR A CÂMERA", "false"]);
  const parados = (await quadros(pagina)).length;
  await pagina.waitForTimeout(400);
  assert.equal((await quadros(pagina)).length, parados, "a câmera desligada continuou mandando quadros");

  // Ligada de novo, e o servidor a encerra: a câmera apaga e diz.
  await clicar(pagina);
  await pagina.waitForFunction(() => document.getElementById("operador-camera").dataset.estado === "no-ar");
  await pagina.evaluate(() => {
    const tela = banca.retrato.minha_transmissao.tela;
    banca.retrato.minha_transmissao = null; banca.retrato.tela = null; banca.retrato.transmissoes = [];
    banca.emitir("ScreenChanged");
    banca.emitir({ ScreenClosed: { screen: tela } });
  });
  await pagina.waitForFunction(() => document.getElementById("operador-camera").dataset.estado === "desligada");
  assert.match((await estado(pagina)).texto, /^A CÂMERA FOI DESLIGADA\./);
  assert.equal((await previa(pagina)).temMidia, false, "a câmera ficou acesa depois de o servidor encerrar a transmissão");

  // E o núcleo que responde que não há mais câmera: o mesmo fim, dito.
  await clicar(pagina);
  await pagina.waitForFunction(() => document.getElementById("operador-camera").dataset.estado === "no-ar");
  await pagina.evaluate(() => { banca.falharOQuadro = "CameraNotLive"; });
  await pagina.waitForFunction(() => document.getElementById("operador-camera").dataset.estado === "desligada");
  assert.match((await estado(pagina)).texto, /^A CÂMERA FOI DESLIGADA\./);

  assert.deepEqual(erros, [], `a página estourou: ${erros.join(" | ")}`);
  await contexto.close();
}

/** 5: a tela no ar, e a câmera entra no lugar dela — dito antes e depois. */
async function noLugarDaTela(navegador, servidor) {
  const { pagina, contexto, erros } = await abrir(navegador, servidor, { telaNoAr: true });
  assert.equal(await pagina.evaluate(() => document.getElementById("operador-vista").dataset.modo), "parar", "a banca não pôs a tela no ar");
  const b = await botao(pagina);
  assert.equal(b.dica, "ligar a câmera: ela entra no lugar da tela que você está compartilhando");
  await captura(pagina, "camera-com-a-tela-no-ar-chromium-1280x800");
  await clicar(pagina);
  await pagina.waitForFunction(() => document.getElementById("operador-camera").dataset.estado === "no-ar");
  assert.match((await estado(pagina)).texto, /^A CÂMERA ENTROU NO LUGAR DA TELA\.\nUma pessoa mostra uma imagem por vez/);
  assert.equal(await pagina.evaluate(() => document.getElementById("anuncio").textContent.startsWith("A CÂMERA ENTROU NO LUGAR DA TELA.")), true,
    "a troca não foi anunciada a quem não vê a tela");
  await captura(pagina, "camera-no-lugar-da-tela-chromium-1280x800");
  assert.deepEqual(erros, []);
  await contexto.close();
}

/** 6: negada, sem câmera, ocupada, sem codificador, sem captura. */
async function semACamera(navegador, servidor, motor, casos) {
  for (const [nome, opcoes, frase, pedeAoSistema] of casos) {
    const { pagina, contexto, erros } = await abrir(navegador, servidor, opcoes);
    if (opcoes.semCodificador || opcoes.semCamera === "sem-captura") {
      const b = await botao(pagina);
      assert.equal(b.desligado, "true", `${nome}: o controle não diz que está indisponível`);
      assert.match(b.dica, /esta janela não/i);
    }
    await clicar(pagina);
    await pagina.waitForFunction(() => Boolean(olhos.frase()) && document.getElementById("operador-camera").dataset.estado === "desligada");
    const e = await estado(pagina);
    assert.match(e.texto, frase, `${nome}: ${e.texto}`);
    assert.ok(e.texto.includes("\n"), `${nome}: a frase não diz o que fazer na segunda linha`);
    assert.equal(await pagina.evaluate(() => banca.pedidosDeMidia), pedeAoSistema ? 1 : 0, `${nome}: pedidos ao sistema`);
    assert.equal((await chamadasDe(pagina, "compartilhar_camera")).length, 0, `${nome}: o núcleo ouviu um pedido de câmera`);
    assert.equal((await botao(pagina)).apertado, "false");
    assert.equal((await previa(pagina)).temMidia, false);
    await captura(pagina, `camera-${nome}-${motor}-1280x800`);
    assert.deepEqual(erros, [], `${nome}: ${erros.join(" | ")}`);
    await contexto.close();
  }
}

/** 7: o servidor do protocolo 12. */
async function noServidorAntigo(navegador, servidor, motor, medida) {
  const [largura, altura] = medida;
  const { pagina, contexto, erros } = await abrir(navegador, servidor, { protocolo: 12, largura, altura });
  const b = await botao(pagina);
  assert.deepEqual([b.escondido, b.desligado, b.dica], [false, "true", "este servidor é de antes da 1.0 e ainda não tem câmera"]);
  await clicar(pagina);
  await pagina.waitForFunction(() => Boolean(olhos.frase()));
  assert.match((await estado(pagina)).texto, /^ESTE SERVIDOR É DE ANTES DA 1\.0 E AINDA NÃO TEM CÂMERA\.\nA câmera chega quando quem hospeda atualizar o SEELE\./);
  assert.equal(await pagina.evaluate(() => banca.pedidosDeMidia), 0, "num servidor antigo a câmera foi pedida ao sistema");
  assert.equal((await chamadasDe(pagina, "compartilhar_camera")).length + (await chamadasDe(pagina, "codificacao_da_camera")).length, 0);
  assert.equal(await pagina.evaluate(() => banca.quadros.length), 0);
  await captura(pagina, `camera-servidor-antigo-${motor}-${largura}x${altura}`);
  assert.deepEqual(await semEstouro(pagina), { largura: true, altura: true });
  assert.deepEqual(erros, []);
  await contexto.close();
}

/** O núcleo que recusa pelo nome (a segunda parede), e o servidor que não confirma. */
async function recusadaPeloNucleo(navegador, servidor) {
  // O retrato diz 13 e o núcleo responde `ServidorAntigo`: a conexão caiu e voltou noutro servidor.
  let { pagina, contexto, erros } = await abrir(navegador, servidor);
  await pagina.evaluate(() => { banca.protocolo = 12; });
  await clicar(pagina);
  await pagina.waitForFunction(() => Boolean(olhos.frase()) && document.getElementById("operador-camera").dataset.estado === "desligada");
  assert.match((await estado(pagina)).texto, /^ESTE SERVIDOR É DE ANTES DA 1\.0/);
  assert.equal((await previa(pagina)).temMidia, false, "a câmera ficou acesa depois de o núcleo recusar");
  assert.deepEqual(erros, []);
  await contexto.close();

  // O servidor não dá nome à transmissão: a câmera é desligada no prazo, e dita.
  ({ pagina, contexto, erros } = await abrir(navegador, servidor));
  // O prazo é o de verdade, dez segundos: a banca não o encurta.
  await pagina.evaluate(() => { banca.mudo = true; });
  await clicar(pagina);
  await pagina.waitForFunction(() => document.getElementById("operador-camera").dataset.estado === "ligando");
  assert.match((await estado(pagina)).texto, /^A CÂMERA ESTÁ ABRINDO…/);
  await captura(pagina, "camera-abrindo-chromium-1280x800");
  await pagina.waitForFunction(() => document.getElementById("operador-camera").dataset.estado === "desligada", null, { timeout: 14_000 });
  assert.match((await estado(pagina)).texto, /^O SERVIDOR NÃO CONFIRMOU A CÂMERA\./);
  assert.equal((await chamadasDe(pagina, "parar_de_compartilhar")).length, 1, "a câmera que não abriu não avisou o servidor que desistiu");
  assert.equal((await previa(pagina)).temMidia, false);
  assert.deepEqual(erros, []);
  await contexto.close();
}

/**
 * O retrato atrasado não apaga a câmera que acabou de abrir, e o retrato que a
 * viu e deixou de ver a apaga, com a frase.
 *
 * O fluxo abre milissegundos depois de o servidor dar nome à transmissão, e o
 * retrato desenhado nesse instante pode ser o de antes. A regra «o servidor
 * encerrou» só vale depois de um retrato ter mostrado a câmera.
 */
async function oRetratoAtrasado(navegador, servidor) {
  const { pagina, contexto, erros } = await abrir(navegador, servidor);
  await clicar(pagina);
  await pagina.waitForFunction(() => document.getElementById("operador-camera").dataset.estado === "no-ar");
  const sala = () => pagina.evaluate(() => banca.retrato.voice_rooms[0]);
  // Uma câmera nova, ainda sem retrato que a tenha mostrado: zera a marca, e
  // desenha um retrato de antes da transmissão.
  await pagina.evaluate((sala) => { cameraDaJanela.noRetrato = false; desenharORodapeDoOperador(sala, null); }, await sala());
  await pagina.waitForTimeout(150);
  assert.equal((await botao(pagina)).estado, "no-ar", "o retrato atrasado apagou a câmera que acabou de abrir");
  assert.equal((await previa(pagina)).trilhasVivas, 1);
  // O retrato que a mostra, e depois o que deixa de mostrar.
  await pagina.evaluate((sala) => desenharORodapeDoOperador(sala, banca.retrato.minha_transmissao), await sala());
  assert.equal((await botao(pagina)).estado, "no-ar");
  await pagina.evaluate((sala) => {
    banca.retrato.minha_transmissao = null; banca.retrato.tela = null; banca.retrato.transmissoes = [];
    desenharORodapeDoOperador(sala, null);
  }, await sala());
  await pagina.waitForFunction(() => document.getElementById("operador-camera").dataset.estado === "desligada");
  assert.match((await estado(pagina)).texto, /^A CÂMERA FOI DESLIGADA\./);
  assert.equal((await previa(pagina)).temMidia, false);
  assert.deepEqual(erros, []);
  await contexto.close();
}

/** 9: quem assiste lê o tipo do que assiste, e nada antes de sabê-lo. */
async function quemAssiste(navegador, servidor, motor) {
  const { pagina, contexto, erros } = await abrir(navegador, servidor);
  const ler = () => pagina.evaluate(() => ({
    quem: document.getElementById("palco-quem").textContent,
    titulo: document.getElementById("palco-titulo-tipo").textContent,
    botoes: [...document.querySelectorAll("#palco-escolha button")].map((b) => ({ texto: b.textContent, dica: b.title })),
  }));
  // Lia anuncia uma transmissão; o fluxo ainda não abriu.
  await pagina.evaluate(async () => {
    const alheia = { tela: 21, de: 2, e_minha: false, espectadores: 1, parada: null, pedido: null };
    banca.retrato.tela = alheia;
    banca.retrato.transmissoes = [{ tela: 21, de: 2, e_minha: false, assistida: true, espectadores: 1 }];
    banca.emitir("ScreenChanged");
    await atualizar();
    await atualizarChamada();
  });
  let visto = await ler();
  assert.equal(visto.quem, "Lia · AO VIVO", "antes da abertura o palco afirma o que a transmissão mostra");
  assert.equal(visto.titulo, "AO VIVO NESTA SALA");
  assert.ok(visto.botoes.every((b) => !/tela|câmera/i.test(b.dica)), `o botão de ver afirma o tipo antes de saber: ${JSON.stringify(visto.botoes)}`);
  await captura(pagina, `camera-assistindo-sem-tipo-${motor}-1280x800`);

  await pagina.evaluate(async () => {
    banca.emitir({ ScreenOpened: { screen: 21, width: 1280, height: 720, source: "camera" } });
    await new Promise((ok) => setTimeout(ok, 120));
  });
  visto = await ler();
  assert.equal(visto.quem, "Lia · CÂMERA");
  assert.equal(visto.titulo, "CÂMERA");
  await captura(pagina, `camera-assistindo-camera-${motor}-1280x800`);
  // A de outra pessoa não é a minha: a prévia não aparece, e o palco arma o decodificador.
  assert.equal((await previa(pagina)).escondida, true, "a câmera de outra pessoa apareceu na prévia de quem assiste");
  assert.equal((await chamadasDe(pagina, "pedir_quadro_chave")).some((a) => a.tela === 21), true, "o palco não pediu a chave da câmera alheia");

  await pagina.evaluate(async () => {
    banca.emitir({ ScreenClosed: { screen: 21 } });
    banca.emitir({ ScreenOpened: { screen: 21, width: 1280, height: 720, source: "monitor" } });
    await new Promise((ok) => setTimeout(ok, 120));
  });
  visto = await ler();
  assert.equal(visto.quem, "Lia · TELA");
  assert.equal(visto.titulo, "TELA COMPARTILHADA");
  assert.deepEqual(erros, []);
  await contexto.close();
}

/**
 * O caminho da **página do navegador**: a mídia da câmera entra no transmissor
 * da tela (`criarATransmissaoDoNavegador`, o de produção), com a sessão e o
 * `.wasm` de mentira no lugar da ponte. Mede-se o que sai dele: o pedido, a
 * fonte declarada, os quadros escritos no fluxo e os eventos da casca.
 */
async function pelaPaginaDoNavegador(navegador, servidor) {
  const { pagina, contexto, erros } = await abrir(navegador, servidor);
  const medido = await pagina.evaluate(async () => {
    const rodar = async (camera) => {
      const chamadas = [], eventos = [], escritos = [];
      const memoria = new WebAssembly.Memory({ initial: 1 });
      const wasm = {
        memory: memoria,
        tela_quadro(tipo, tamanho) { new Uint8Array(memoria.buffer).set([tipo, (tamanho >>> 24) & 255, (tamanho >>> 16) & 255, (tamanho >>> 8) & 255, tamanho & 255]); return 0; },
        saida_ptr: () => 0,
        saida_len: () => 5,
      };
      const sessao = {
        atender(comando, args) {
          chamadas.push({ comando, args: JSON.parse(JSON.stringify(args ?? {})) });
          if (comando === "codificacao_da_tela") return { resposta: { codec: "avc1.42E028", largura: 640, altura: 360, quadros: 30, banda_bps: 800_000 } };
          if (comando === "abertura_da_tela") return { resposta: [0xAB, 0xCD] };
          return { resposta: null };
        },
        abrirUmFluxo: async () => new WritableStream({ write(bytes) { escritos.push(Array.from(bytes.subarray(0, 5))); } }),
      };
      const transmissao = criarATransmissaoDoNavegador({ sessao, wasm, eu: () => 1, emitir: (evento) => eventos.push(Object.keys(evento)[0] + (evento.ScreenOpened ? ":" + evento.ScreenOpened.source : "")) });
      const midia = await navigator.mediaDevices.getUserMedia(PEDIDO_DA_CAMERA);
      transmissao.comecar(midia, { ...LIMITES_DA_CAMERA }, camera ? { camera: true } : undefined);
      transmissao.aoDobrar({ ScreenShareStarted: { person: 1, screen: 5 } });
      const espera = async (pronto) => { for (let i = 0; i < 400 && !pronto(); i++) await new Promise((ok) => setTimeout(ok, 25)); return pronto(); };
      const chegou = await espera(() => escritos.length >= 8);
      const antes = escritos.length;
      transmissao.aoDobrar({ KeyFrameRequested: { screen: 5 } });
      const pedida = await espera(() => escritos.slice(antes).some((q) => q[0] === 1));
      const dica = midia.getVideoTracks()[0].contentHint;
      const visto = { chegou, pedida, dica, eCamera: transmissao.camera, aMidiaEEla: transmissao.midia === midia };
      transmissao.parar();
      return { chamadas, eventos, escritos: escritos.slice(0, 6), solta: midia.getVideoTracks().every((t) => t.readyState === "ended"), ...visto };
    };
    return { camera: await rodar(true), tela: await rodar(false) };
  });
  const { camera, tela } = medido;
  assert.ok(camera.chegou, `os quadros da câmera não foram escritos no fluxo: ${JSON.stringify(camera.escritos)}`);
  assert.deepEqual(camera.chamadas.map((c) => c.comando).slice(0, 3), ["compartilhar_camera", "codificacao_da_tela", "abertura_da_tela"]);
  assert.equal(camera.chamadas[1].args.fonte, "camera", "a codificação não foi pedida como a de uma câmera");
  assert.equal(camera.chamadas[2].args.fonte, "camera", "a abertura não declara a câmera");
  assert.deepEqual(camera.escritos[0].slice(0, 2), [0xAB, 0xCD], "o primeiro escrito no fluxo não é a abertura");
  assert.equal(camera.escritos[1][0], 1, "o primeiro quadro da câmera não é chave");
  assert.ok(camera.escritos.slice(2).some((q) => q[0] === 0), "depois da chave não saiu quadro comum");
  assert.ok(camera.pedida, "o `KeyFrameRequested` não fez a câmera mandar uma chave");
  assert.equal(camera.dica, "motion");
  assert.deepEqual([camera.eCamera, camera.aMidiaEEla, camera.solta], [true, true, true]);
  assert.ok(camera.eventos.includes("ScreenOpened:camera"), `a casca não ouviu que a câmera abriu: ${camera.eventos.slice(0, 4)}`);
  assert.ok(!camera.eventos.includes("ScreenFrame"), "a página mandou à casca os quadros da própria câmera");
  // E o outro lado, para o de cima não passar por ser sempre assim: sem a
  // marca de câmera, a mesma mídia sai como tela, com o espelho.
  assert.equal(tela.chamadas[0].comando, "compartilhar_tela");
  assert.equal(tela.chamadas[2].args.fonte, "monitor");
  assert.ok(tela.eventos.includes("ScreenOpened:monitor") && tela.eventos.includes("ScreenFrame"));
  assert.equal(tela.eCamera, false);
  assert.notEqual(tela.dica, "motion");
  assert.deepEqual(erros, []);
  await contexto.close();
}

async function principal() {
  const servidor = servir();
  await new Promise((ok) => servidor.listen(0, "127.0.0.1", ok));
  const chromium = await motores.chromium().launch({
    args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream"],
  });
  const feitos = [];
  const rodar = async (nome, caso) => {
    try {
      await caso();
      feitos.push(nome);
      console.log(`  ✓ ${nome}`);
    } catch (falha) {
      if (process.env.SEELE_PREVIEW_DIR && paginaEmCurso) {
        await paginaEmCurso.screenshot({ path: path.join(process.env.SEELE_PREVIEW_DIR, `FALHA-${nome.replace(/[^a-z0-9]+/gi, "-")}.png`) }).catch(() => {});
      }
      throw new Error(`${nome}: ${falha.stack ?? falha}`);
    }
  };
  try {
    await rodar("a câmera no ar, 1280×800", () => aCameraNoAr(chromium, servidor, [1280, 800]));
    await rodar("a câmera no ar, 720×480", () => aCameraNoAr(chromium, servidor, [720, 480]));
    await rodar("a câmera no lugar da tela", () => noLugarDaTela(chromium, servidor));
    await rodar("negada, sem câmera, ocupada, sem codificador, sem captura", () => semACamera(chromium, servidor, "chromium", [
      ["negada", { semCamera: "NotAllowedError" }, /^O SISTEMA NÃO DEU A CÂMERA AO SEELE\./, true],
      ["sem-camera", { semCamera: "NotFoundError" }, /^NENHUMA CÂMERA FOI ENCONTRADA\./, true],
      ["ocupada", { semCamera: "NotReadableError" }, /^A CÂMERA NÃO ABRIU\./, true],
      ["sem-codificador", { semCodificador: true }, /^ESTA JANELA NÃO CODIFICA VÍDEO DE CÂMERA\./, false],
      ["sem-captura", { semCamera: "sem-captura" }, /^ESTA JANELA NÃO TEM ACESSO A CÂMERAS\./, false],
    ]));
    await rodar("o servidor do protocolo 12, 1280×800", () => noServidorAntigo(chromium, servidor, "chromium", [1280, 800]));
    await rodar("o servidor do protocolo 12, 720×480", () => noServidorAntigo(chromium, servidor, "chromium", [720, 480]));
    await rodar("recusada pelo núcleo, e sem confirmação do servidor", () => recusadaPeloNucleo(chromium, servidor));
    await rodar("quem assiste lê o tipo", () => quemAssiste(chromium, servidor, "chromium"));
    await rodar("o retrato atrasado não apaga a câmera", () => oRetratoAtrasado(chromium, servidor));
    await rodar("a página do navegador: a câmera no transmissor da tela", () => pelaPaginaDoNavegador(chromium, servidor));
  } finally {
    await chromium.close();
  }

  // O WebKit do Playwright não tem câmera de mentira: roda o que não precisa
  // dela — o servidor antigo, as recusas do sistema e os rótulos de quem assiste.
  if (process.env.SEELE_CAMERA_WEBKIT) {
    const webkit = await motores.webkit().launch();
    try {
      await rodar("WebKit: o servidor do protocolo 12", () => noServidorAntigo(webkit, servidor, "webkit", [1280, 800]));
      await rodar("WebKit: negada e sem câmera", () => semACamera(webkit, servidor, "webkit", [
        ["negada", { semCamera: "NotAllowedError" }, /^O SISTEMA NÃO DEU A CÂMERA AO SEELE\./, true],
        ["sem-camera", { semCamera: "NotFoundError" }, /^NENHUMA CÂMERA FOI ENCONTRADA\./, true],
      ]));
      await rodar("WebKit: quem assiste lê o tipo", () => quemAssiste(webkit, servidor, "webkit"));
    } finally {
      await webkit.close();
    }
  }
  servidor.close();
  console.log(`camera: ${feitos.length} casos, tudo passou`);
}

principal().then(() => process.exit(0), (falha) => {
  console.error(falha);
  process.exit(1);
});
