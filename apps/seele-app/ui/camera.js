// A câmera (1.0, protocolo 13): no lugar da tela, pelo mesmo caminho dela.
//
// # O que este arquivo é
//
// O controle CÂMERA da barra do operador, os estados dele, e a metade da
// câmera que só a janela alcança: o `getUserMedia`, a imagem que quem liga vê
// de si, e — no aplicativo — o `VideoEncoder` que entrega os quadros ao núcleo.
//
// # Dois caminhos, uma tela
//
// - **Na página do navegador**, quem transmite é a `compartilhar-web.js`: a
//   mídia da câmera entra no mesmo codificador e no mesmo fluxo da tela, e
//   daqui sai só o `getUserMedia` (o `compartilhar_camera` da ponte vem buscar
//   a mídia em `midiaDaCameraDaJanela`).
// - **No aplicativo**, o núcleo não tem captura de câmera — o workspace proíbe
//   `unsafe`, e não há câmera nativa sem ele. Esta janela captura e codifica,
//   e cada quadro vai ao núcleo pelo `quadro_da_camera`, em bytes crus. O
//   núcleo abre o fluxo, guarda o teto e fatia o quadro-chave, como faz com a
//   tela; e devolve três pedidos por evento: um quadro-chave
//   (`CameraKeyFrameRequested`), a banda de agora (`CameraBudget`) e a parada
//   pelo sinal (`CameraPaused`).
//
// # O que fica dito, e não escondido
//
// Nenhum estado é mudo: pedindo a permissão, negada (com o que fazer), sem
// câmera, sem codificador nesta janela, no ar, parada pelo sinal, a fila que
// não acompanha, e o servidor de antes da 1.0. As frases são de `frases.js`
// (`FRASES_DA_CAMERA`).
//
// A imagem que quem liga vê de si vem da própria mídia, num `<video>`: ela não
// vai ao núcleo e volta.

/**
 * **O número de protocolo em que a câmera nasceu.** O retrato traz o número
 * que a conexão negociou (`protocolo`), e o controle não oferece o que o
 * servidor não conhece: num servidor da 0.19 ele fica à vista, apagado, com o
 * porquê. O número é do `seele-proto` (`screen::PROTOCOLO_DA_CAMERA`), e o
 * guarda `o_numero_da_camera_e_o_do_protocolo` cobra que este seja o mesmo.
 */
const PROTOCOLO_DA_CAMERA = 13;

/** Se o servidor deste retrato conhece a câmera. */
function servidorTemACamera(snapshot) {
  return Number(snapshot?.protocolo ?? 0) >= PROTOCOLO_DA_CAMERA;
}

/**
 * O que se pede ao `getUserMedia`: no máximo 720p a 30, sem som — a voz de
 * quem fala já vai pela voz. São ideais, e não exigências: uma câmera que só
 * entrega menos entrega o que tem, e o codificador nunca amplia.
 */
const PEDIDO_DA_CAMERA = Object.freeze({
  video: Object.freeze({
    width: Object.freeze({ ideal: 1280 }),
    height: Object.freeze({ ideal: 720 }),
    frameRate: Object.freeze({ ideal: 30, max: 30 }),
  }),
  audio: false,
});

/** Os limites da câmera, na forma dos da tela: 720p a 30, movimento, sem som. */
const LIMITES_DA_CAMERA = Object.freeze({
  banda_bps: null,
  altura_maxima: 720,
  quadros_maximos: 30,
  prioridade: "movimento",
  com_som: false,
});

/**
 * Quanto do teto do fluxo o codificador mira. O teto é do fluxo inteiro, e um
 * codificador que mira exatamente nele o estoura a cada quadro mais gordo — o
 * que estoura é descartado antes do fio, e cada descarte custa um quadro-chave.
 * Não foi medido em WKWebView nem em WebView2: é a folga de partida.
 */
const FRACAO_DO_TETO_DA_CAMERA = 0.85;

/** Quantos quadros a captura pula depois de um quadro-chave, que sai em fatias. */
const QUADROS_APOS_A_CHAVE = 3;

/**
 * Com quantos quadros na fila do núcleo a captura deixa de codificar: a metade
 * da fila (`FILA_DE_FORA` = 8, em `seele-core/src/de_fora.rs`).
 *
 * O `quadro_da_camera` devolve quantos quadros estão na fila. Um quadro
 * **recusado** já foi codificado: o seguinte foi predito dele, e a recusa
 * custa um quadro-chave. Um quadro que **não foi codificado** não custa nada.
 * Parar na metade é a contrapressão que a tela do sistema tem (a bomba dela
 * espera em vez de codificar) — medido em 10/11/2026, a recusa seguida de
 * chave era 27 pontos dos 43% de tempo congelado a 1% de perda.
 */
const LIMIAR_DA_FILA_DA_CAMERA = 4;

/**
 * De quanto em quanto tempo, com a fila acima do limiar, a captura manda um
 * quadro mesmo assim. O número da fila só chega na resposta de um quadro: sem
 * esta sonda, a captura que parou nunca saberia que a fila esvaziou.
 */
const SONDA_DA_FILA_DA_CAMERA_MS = 100;

/** O prazo para o servidor dar nome à câmera depois do pedido. */
const PRAZO_DA_CAMERA_MS = 10_000;

/**
 * Se esta é a casca do celular. Lá a câmera não é oferecida ainda: a ponte
 * recusa o pedido com a frase (`SO_NA_CASCA_LARGA`), e o controle nem aparece
 * — um botão que pede a câmera ao sistema para depois recusar gastaria a
 * pergunta de permissão com nada.
 */
function cameraSoNoPc() {
  return PONTE_NO_LUGAR_DO_TAURI && typeof CASCA_DA_PONTE !== "undefined" && CASCA_DA_PONTE === "celular";
}

/** Se esta janela tem como capturar a câmera e codificá-la. */
function temCameraParaCodificar() {
  return Boolean(navigator.mediaDevices?.getUserMedia) && typeof VideoEncoder === "function" &&
    typeof VideoFrame === "function" && temLeituraDeVideoDaTela();
}

// ------------------------------------------------- o tipo do que se assiste

/** O tipo de cada transmissão aberta, pelo `source` do `ScreenOpened`. */
const tiposDasTransmissoes = new Map();

/** O tipo desta transmissão — `"camera"`, `"monitor"`, `"window"` — ou `null`. */
function tipoDaTransmissao(tela) {
  return tiposDasTransmissoes.get(tela) ?? null;
}

/**
 * As palavras de uma transmissão, pelo tipo dela. Antes de a abertura do
 * fluxo chegar não se sabe o que ela mostra, e nenhuma das frases afirma
 * «tela» nem «câmera».
 */
function palavrasDaTransmissao(tela) {
  const tipo = tipoDaTransmissao(tela);
  const f = FRASES_DA_CAMERA;
  return {
    tipo,
    rotulo: f.rotulos[tipo] ?? f.rotuloSemTipo,
    titulo: f.titulos[tipo] ?? f.tituloSemTipo,
    sua: f.sua[tipo] ?? f.suaSemTipo,
    ver: f.verDe[tipo] ?? f.verSemTipo,
  };
}

// ------------------------------------------------------------- a câmera

/**
 * A câmera desta janela.
 *
 * `estado`: `desligada`, `pedindo` (a permissão), `ligando` (o servidor ainda
 * não deu nome), `no-ar`, `parada` (pelo sinal; volta sozinha). `frase` é o
 * que o controle escreve ao lado dele, e fica até o próximo gesto.
 */
const cameraDaJanela = {
  estado: "desligada",
  midia: null,
  tela: null,
  frase: "",
  recusados: 0,
  /** O carimbo da sessão em que ela foi ligada: os quadros vão a ela. */
  carimbo: null,
  codificacao: null,
  codificador: null,
  leitor: null,
  chave: true,
  pular: 0,
  banda: 0,
  enviando: 0,
  /** Quantos quadros o núcleo tinha na fila na última resposta, e quando. */
  naFila: 0,
  naFilaEm: 0,
  prazo: null,
  vez: 0,
  /** Um retrato já mostrou esta câmera como a transmissão desta pessoa. */
  noRetrato: false,
};

/** Se a câmera desta janela está pedida, abrindo, no ar ou parada pelo sinal. */
function cameraLigada() {
  return cameraDaJanela.estado !== "desligada";
}

/** Se esta transmissão é a câmera desta janela. */
function ehAMinhaCamera(tela) {
  return cameraLigada() && cameraDaJanela.tela === tela;
}

function escreverEstadoDaCamera(estado, frase = "") {
  cameraDaJanela.estado = estado;
  cameraDaJanela.frase = frase;
  desenharACamera();
}

/** Solta a câmera, o codificador e a prévia. Não fala com o servidor. */
function soltarACamera() {
  cameraDaJanela.vez += 1;
  clearTimeout(cameraDaJanela.prazo);
  cameraDaJanela.prazo = null;
  cameraDaJanela.leitor?.cancel?.().catch?.(() => {});
  cameraDaJanela.leitor = null;
  const codificador = cameraDaJanela.codificador;
  cameraDaJanela.codificador = null;
  if (codificador && codificador.state !== "closed") codificador.close();
  for (const trilha of cameraDaJanela.midia?.getTracks() ?? []) trilha.stop();
  cameraDaJanela.midia = null;
  cameraDaJanela.tela = null;
  cameraDaJanela.carimbo = null;
  cameraDaJanela.codificacao = null;
  cameraDaJanela.recusados = 0;
  cameraDaJanela.enviando = 0;
  cameraDaJanela.naFila = 0;
  cameraDaJanela.naFilaEm = 0;
  cameraDaJanela.noRetrato = false;
}

/**
 * Desliga: a câmera solta, o servidor fica sabendo, e a frase — se há uma —
 * fica escrita ao lado do controle.
 */
async function desligarACamera(frase = "") {
  const estava = cameraLigada();
  soltarACamera();
  escreverEstadoDaCamera("desligada", frase);
  if (!estava) return;
  try {
    await naFilaDeTela(() => invoke("parar_de_compartilhar"));
  } catch (falha) {
    if (falha !== "NotConnected") console.warn("câmera: o servidor não ouviu o fim:", falha);
  }
  if (typeof atualizarChamada === "function") await atualizarChamada().catch(() => {});
}

/** Liga: a permissão, a mídia, e o pedido ao servidor. */
async function ligarACamera() {
  if (cameraLigada() || cameraSoNoPc()) return;
  const retrato = typeof desenhado !== "undefined" ? desenhado : null;
  if (!servidorTemACamera(retrato)) {
    escreverEstadoDaCamera("desligada", FRASES_DA_CAMERA.servidorAntigo);
    return;
  }
  if (!navigator.mediaDevices?.getUserMedia) {
    escreverEstadoDaCamera("desligada", FRASES_DA_CAMERA.semCaptura);
    return;
  }
  if (!temCameraParaCodificar()) {
    escreverEstadoDaCamera("desligada", FRASES_DA_CAMERA.semCodificador);
    return;
  }
  const noLugarDaTela = Boolean(desenharACamera.minha ?? retrato?.minha_transmissao);
  const vez = ++cameraDaJanela.vez;
  escreverEstadoDaCamera("pedindo", FRASES_DA_CAMERA.pedindo);
  let midia;
  try {
    // Dentro do gesto: nada espera antes da pergunta do sistema.
    midia = await navigator.mediaDevices.getUserMedia(PEDIDO_DA_CAMERA);
  } catch (falha) {
    console.warn("câmera: o sistema não a entregou:", falha?.name, falha?.message);
    if (vez === cameraDaJanela.vez) escreverEstadoDaCamera("desligada", fraseDaFalhaDaCamera(falha));
    return;
  }
  // DESLIGAR, ou outra tentativa, enquanto a pergunta do sistema estava aberta.
  if (vez !== cameraDaJanela.vez || cameraDaJanela.estado !== "pedindo") {
    for (const trilha of midia.getTracks()) trilha.stop();
    return;
  }
  const trilha = midia.getVideoTracks()[0];
  if (!trilha) {
    for (const outra of midia.getTracks()) outra.stop();
    escreverEstadoDaCamera("desligada", FRASES_DA_CAMERA.semCamera);
    return;
  }
  if ("contentHint" in trilha) trilha.contentHint = "motion";
  cameraDaJanela.midia = midia;
  cameraDaJanela.carimbo = vistaDaJanela.sessao === null ? null : { sessao: vistaDaJanela.sessao, geracao: vistaDaJanela.geracao };
  // A câmera que o sistema tira (o cabo, a tampa, outra aba): a transmissão
  // acaba com ela, e a frase diz.
  trilha.addEventListener("ended", () => {
    if (cameraDaJanela.midia === midia) desligarACamera(FRASES_DA_CAMERA.parou.terminar).catch(() => {});
  });
  escreverEstadoDaCamera("ligando", noLugarDaTela ? FRASES_DA_CAMERA.noLugarDaTela : FRASES_DA_CAMERA.ligando);
  try {
    await pedirACamera(trilha);
  } catch (falha) {
    if (cameraDaJanela.midia !== midia) return;
    soltarACamera();
    escreverEstadoDaCamera("desligada", fraseDoPedidoDaCamera(falha));
    return;
  }
  if (cameraDaJanela.midia !== midia) return;
  // O servidor dá nome à transmissão, e o fluxo abre: o `ScreenOpened` dela é
  // o que põe a câmera no ar. Sem ele no prazo, ela é desligada e dita.
  cameraDaJanela.prazo = setTimeout(() => {
    if (cameraDaJanela.midia === midia && cameraDaJanela.estado === "ligando") {
      desligarACamera(FRASES_DA_CAMERA.semConfirmacao).catch(() => {});
    }
  }, PRAZO_DA_CAMERA_MS);
  if (noLugarDaTela) anunciar(FRASES_DA_CAMERA.noLugarDaTela);
}

/** A frase de um pedido de câmera que o núcleo ou a ponte recusaram. */
function fraseDoPedidoDaCamera(falha) {
  if (falha?.ServidorAntigo) return FRASES_DA_CAMERA.servidorAntigo;
  if (falha?.semCodificador) return FRASES_DA_CAMERA.semCodificador;
  if (falha?.RecusaDaPonte?.frase) return falha.RecusaDaPonte.frase;
  return fraseDeErro(falha);
}

/**
 * A mídia da câmera desta janela, para a ponte do navegador: lá quem codifica
 * e escreve é a transmissão da página (`compartilhar-web.js`), e o
 * `compartilhar_camera` da ponte vem buscá-la aqui. Uma mídia não atravessa
 * um `invoke` — no aplicativo ele é JSON —, e o comando tem os mesmos
 * argumentos nos dois lugares.
 */
function midiaDaCameraDaJanela() {
  return cameraDaJanela.midia;
}

/**
 * As medidas e o codec vêm de quem escreve a abertura (`codificacao_da_camera`:
 * o núcleo no aplicativo, o `.wasm` na página), o codificador desta janela é
 * posto à prova com um quadro de verdade, e só então o pedido sai. Um
 * codificador que não codifica recusa aqui, antes de a sala ouvir que há uma
 * câmera.
 */
async function pedirACamera(trilha) {
  const ajustes = trilha.getSettings();
  const codificacao = await invoke("codificacao_da_camera", {
    largura: ajustes.width ?? 0,
    altura: ajustes.height ?? 0,
    limites: LIMITES_DA_CAMERA,
  });
  const configuracao = configuracaoDaCamera(codificacao, codificacao.banda_bps);
  let suportada = false;
  try {
    suportada = (await VideoEncoder.isConfigSupported(configuracao)).supported === true &&
      await sondarOCodificadorDeVideo(codificacao.codec);
  } catch (falha) {
    console.warn("câmera: a pergunta ao codificador falhou:", falha);
  }
  if (!suportada) throw { semCodificador: true };
  cameraDaJanela.codificacao = codificacao;
  cameraDaJanela.banda = codificacao.banda_bps;
  await naFilaDeTela(() => invoke("compartilhar_camera", {
    largura: codificacao.largura,
    altura: codificacao.altura,
    limites: LIMITES_DA_CAMERA,
  }));
}

/** A configuração do `VideoEncoder` da câmera, com esta banda de fluxo. */
function configuracaoDaCamera(codificacao, tetoBps) {
  return {
    codec: codificacao.codec,
    width: codificacao.largura,
    height: codificacao.altura,
    bitrate: Math.max(100_000, Math.floor(tetoBps * FRACAO_DO_TETO_DA_CAMERA)),
    framerate: codificacao.quadros,
    latencyMode: "realtime",
    avc: { format: "annexb" },
  };
}

// ------------------------------------- o codificador da câmera (aplicativo)

/** Um quadro codificado vai ao núcleo, em bytes crus, pelo carimbo da câmera. */
async function mandarOQuadroDaCamera(chave, bytes, vez) {
  const carimbo = cameraDaJanela.carimbo;
  const cabecalhos = { "seele-chave": chave ? "1" : "0" };
  if (carimbo) {
    cabecalhos["seele-sessao"] = String(carimbo.sessao);
    cabecalhos["seele-geracao"] = String(carimbo.geracao);
  }
  cameraDaJanela.enviando += 1;
  try {
    const naFila = await invocarNaJanela("quadro_da_camera", bytes, { headers: cabecalhos });
    if (vez === cameraDaJanela.vez) {
      cameraDaJanela.naFila = Number(naFila) || 0;
      cameraDaJanela.naFilaEm = performance.now();
    }
  } catch (falha) {
    if (vez !== cameraDaJanela.vez) return;
    // O quadro que não foi era referência do seguinte: o próximo é chave.
    cameraDaJanela.chave = true;
    if (falha?.CameraQueueFull) {
      cameraDaJanela.recusados = falha.CameraQueueFull.dropped;
      cameraDaJanela.frase = FRASES_DA_CAMERA.filaCheia(cameraDaJanela.recusados);
      desenharACamera();
      return;
    }
    console.warn("câmera: o núcleo recusou um quadro:", falha);
    // Sem transmissão do outro lado (ela acabou, a conexão caiu e voltou): a
    // câmera não fica acesa para ninguém.
    await desligarACamera(falha === "CameraNotLive" || falha?.SessaoNaoConfere || falha === "NotConnected"
      ? FRASES_DA_CAMERA.encerrada
      : fraseDeErro(falha));
  } finally {
    cameraDaJanela.enviando -= 1;
  }
}

/** Arma o codificador com a banda de agora; o primeiro quadro é chave. */
function armarOCodificadorDaCamera(vez) {
  const anterior = cameraDaJanela.codificador;
  if (anterior && anterior.state !== "closed") anterior.close();
  const codificador = new VideoEncoder({
    output: (pedaco) => {
      if (cameraDaJanela.codificador !== codificador || vez !== cameraDaJanela.vez) return;
      mandarOQuadroDaCamera(pedaco.type === "key", bytesDoPedaco(pedaco), vez);
    },
    error: (falha) => {
      if (cameraDaJanela.codificador !== codificador || vez !== cameraDaJanela.vez) return;
      console.warn("câmera: o codificador parou:", falha);
      desligarACamera(FRASES_DA_CAMERA.parou.codificar).catch(() => {});
    },
  });
  codificador.configure(configuracaoDaCamera(cameraDaJanela.codificacao, cameraDaJanela.banda));
  cameraDaJanela.codificador = codificador;
  cameraDaJanela.chave = true;
  cameraDaJanela.pular = 0;
}

/**
 * Se a fila do núcleo passou do limiar e ainda não é hora de sondá-la.
 *
 * A última resposta do `quadro_da_camera` diz quantos quadros esperavam. Acima
 * do limiar a captura não codifica — a não ser um quadro a cada
 * `SONDA_DA_FILA_DA_CAMERA_MS`, que é como ela fica sabendo que a fila andou.
 */
function filaDoNucleoCheia() {
  return cameraDaJanela.naFila >= LIMIAR_DA_FILA_DA_CAMERA &&
    performance.now() - cameraDaJanela.naFilaEm < SONDA_DA_FILA_DA_CAMERA_MS;
}

/** Lê a câmera e codifica, na cadência e nas medidas pedidas. */
async function codificarACamera(vez) {
  const { largura, altura, quadros } = cameraDaJanela.codificacao;
  const trilha = cameraDaJanela.midia.getVideoTracks()[0];
  const leitor = typeof MediaStreamTrackProcessor === "function"
    ? new MediaStreamTrackProcessor({ track: trilha }).readable.getReader()
    : leitorDeVideoSemProcessador(trilha);
  cameraDaJanela.leitor = leitor;
  const tela = new OffscreenCanvas(largura, altura);
  const contexto = tela.getContext("2d");
  const intervaloUs = 1_000_000 / quadros;
  let ultimo = -Infinity;
  for (;;) {
    let lido;
    try {
      lido = await leitor.read();
    } catch (falha) {
      console.warn("câmera: a leitura parou:", falha);
      if (vez === cameraDaJanela.vez) desligarACamera(FRASES_DA_CAMERA.parou.continuar).catch(() => {});
      return;
    }
    if (vez !== cameraDaJanela.vez) {
      lido.value?.close();
      return;
    }
    if (lido.done) {
      desligarACamera(FRASES_DA_CAMERA.parou.terminar).catch(() => {});
      return;
    }
    const original = lido.value;
    const codificador = cameraDaJanela.codificador;
    const agora = original.timestamp;
    // Parada pelo sinal, fora da cadência, ou com uma das filas cheia: o
    // quadro não entra no codificador. **E o próximo não precisa ser chave**:
    // o que ficou de fora aqui nunca foi codificado, e nenhum quadro foi
    // predito dele. Chave é para o quadro codificado que se perdeu
    // (`mandarOQuadroDaCamera`).
    const cheia = cameraDaJanela.enviando > 4 || (codificador?.encodeQueueSize ?? 0) > 2 ||
      filaDoNucleoCheia();
    if (!codificador || cameraDaJanela.estado !== "no-ar" || agora - ultimo < intervaloUs || cheia) {
      original.close();
      continue;
    }
    ultimo = agora;
    // O quadro-chave sai em fatias, uma por intervalo de quadro: a captura
    // espera as fatias acabarem em vez de pôr um quadro no meio delas.
    if (cameraDaJanela.pular > 0 && !cameraDaJanela.chave) {
      cameraDaJanela.pular -= 1;
      original.close();
      continue;
    }
    const fator = Math.min(largura / original.displayWidth, altura / original.displayHeight);
    const w = original.displayWidth * fator, h = original.displayHeight * fator;
    contexto.fillStyle = "black";
    contexto.fillRect(0, 0, largura, altura);
    contexto.drawImage(original, (largura - w) / 2, (altura - h) / 2, w, h);
    original.close();
    const quadro = new VideoFrame(tela, { timestamp: agora });
    const chave = cameraDaJanela.chave;
    try {
      codificador.encode(quadro, { keyFrame: chave });
      cameraDaJanela.chave = false;
      if (chave) cameraDaJanela.pular = QUADROS_APOS_A_CHAVE;
    } catch (falha) {
      console.warn("câmera: um quadro não entrou no codificador:", falha);
      desligarACamera(FRASES_DA_CAMERA.parou.codificar).catch(() => {});
      return;
    } finally {
      quadro.close();
    }
  }
}

// --------------------------------------------------------------- os eventos

/** A câmera desta janela abriu (ou reabriu, depois de uma parada). */
function aCameraAbriu(tela) {
  clearTimeout(cameraDaJanela.prazo);
  cameraDaJanela.prazo = null;
  cameraDaJanela.tela = tela;
  const frase = cameraDaJanela.frase === FRASES_DA_CAMERA.noLugarDaTela ? cameraDaJanela.frase : FRASES_DA_CAMERA.noAr;
  if (!PONTE_NO_LUGAR_DO_TAURI) {
    const vez = cameraDaJanela.vez;
    const primeira = cameraDaJanela.codificador === null && cameraDaJanela.leitor === null;
    armarOCodificadorDaCamera(vez);
    escreverEstadoDaCamera("no-ar", frase);
    if (primeira) codificarACamera(vez).catch((falha) => console.warn("câmera: a codificação parou:", falha));
  } else {
    escreverEstadoDaCamera("no-ar", frase);
  }
  if (typeof atualizarChamada === "function") atualizarChamada().catch(() => {});
}

/**
 * Se esta abertura é a da câmera que esta janela pediu: a primeira de uma
 * câmera, enquanto ela abre, que o retrato confirma como a transmissão desta
 * pessoa; ou a reabertura da mesma, depois de uma parada pelo sinal.
 */
async function aAberturaEDaMinhaCamera(tela) {
  if (!cameraLigada()) return false;
  if (cameraDaJanela.tela !== null) return cameraDaJanela.tela === tela;
  if (cameraDaJanela.estado !== "ligando") return false;
  const midia = cameraDaJanela.midia;
  let retrato = null;
  try { retrato = await invoke("snapshot"); } catch { /* sem sessão: não é a minha */ }
  return cameraDaJanela.midia === midia && retrato?.minha_transmissao?.tela === tela;
}

listen("seele://event", (evento) => {
  const carga = evento.payload;
  if (!carga || typeof carga !== "object") return;
  if (carga.ScreenOpened) {
    const { screen, source } = carga.ScreenOpened;
    if (typeof source === "string") tiposDasTransmissoes.set(screen, source);
    if (source === "camera") {
      aAberturaEDaMinhaCamera(screen).then((minha) => { if (minha) aCameraAbriu(screen); });
    }
    if (typeof atualizarChamada === "function") atualizarChamada().catch(() => {});
  } else if (carga.ScreenClosed) {
    const { screen } = carga.ScreenClosed;
    tiposDasTransmissoes.delete(screen);
    // O fluxo da própria câmera fechou sem ninguém ter desligado: o servidor a
    // encerrou, ou a conexão caiu. Parada pelo sinal não passa por aqui.
    if (ehAMinhaCamera(screen)) {
      desligarACamera(carga.ScreenClosed.erro ?? FRASES_DA_CAMERA.encerrada).catch(() => {});
    }
  } else if (carga.CameraKeyFrameRequested) {
    if (ehAMinhaCamera(carga.CameraKeyFrameRequested.screen)) cameraDaJanela.chave = true;
  } else if (carga.CameraBudget) {
    const { screen, bps } = carga.CameraBudget;
    if (cameraDaJanela.tela !== null && !ehAMinhaCamera(screen)) return;
    if (cameraDaJanela.banda === bps) return;
    cameraDaJanela.banda = bps;
    const codificador = cameraDaJanela.codificador;
    if (codificador && codificador.state === "configured") {
      // **Sem pedir quadro-chave.** Trocar a banda não troca as medidas nem
      // o codec, e quem assiste segue decodificando a mesma cadeia. A chave
      // pedida aqui a cada troca era uma rajada por mudança de orçamento, e
      // num caminho estreito a rajada derrubava a estimativa que acabara de
      // subir (medido em 11/11/2026 a 1 Mbps: 13 chaves e 370 kbps em 40 s com
      // ela, 1 chave e 526 kbps sem). Se o codificador deste navegador
      // precisar recomeçar de uma imagem inteira, ele mesmo a emite, e o
      // `pedaco.type` a anuncia como chave.
      codificador.configure(configuracaoDaCamera(cameraDaJanela.codificacao, bps));
    }
  } else if (carga.CameraPaused) {
    const { screen, reason } = carga.CameraPaused;
    if (cameraDaJanela.tela !== null && !ehAMinhaCamera(screen)) return;
    if (!cameraLigada()) return;
    clearTimeout(cameraDaJanela.prazo);
    cameraDaJanela.prazo = null;
    cameraDaJanela.tela = screen;
    escreverEstadoDaCamera("parada", FRASES_DA_CAMERA.paradas[reason] ?? FRASES_DA_CAMERA.paradaDesconhecida);
  } else if (carga.ScreenShareFell) {
    // A transmissão da câmera caiu, e o núcleo já a encerrou. O `ScreenClosed`
    // dela chega antes e já a desligou com a frase de quem não sabe o motivo
    // («acabou do lado do servidor, ou a conexão caiu»); este traz o motivo,
    // e a frase passa a dizê-lo.
    const { screen, source, reason } = carga.ScreenShareFell;
    if (source !== "camera") return;
    const frase = fraseDaQuedaDaTransmissao("camera", reason);
    if (ehAMinhaCamera(screen)) desligarACamera(frase).catch(() => {});
    else if (!cameraLigada() && cameraDaJanela.frase === FRASES_DA_CAMERA.encerrada) escreverEstadoDaCamera("desligada", frase);
  }
});

// ---------------------------------------------------------------- o desenho

/**
 * O controle e o estado, pelo que a câmera e o retrato dizem.
 *
 * Chamado a cada retrato (`desenharORodapeDoOperador`) e a cada mudança de
 * estado daqui. O estado mora no botão, como o do COMPARTILHAR: `data-estado`,
 * `aria-pressed`, e o `aria-disabled` com o porquê no `title`.
 */
function desenharACamera(voice_room = undefined, minha = undefined) {
  const botao = $("operador-camera");
  const estado = $("camera-estado");
  if (!botao || !estado) return;
  // Com argumentos é o desenho de um retrato (`desenharORodapeDoOperador`): a
  // sala e a transmissão desta pessoa, como ele as trouxe. Sem argumentos é
  // uma mudança de estado daqui, que redesenha com o que o último retrato disse.
  const doRetrato = voice_room !== undefined;
  if (doRetrato) {
    desenharACamera.sala = voice_room;
    desenharACamera.minha = minha ?? null;
  }
  voice_room = desenharACamera.sala ?? null;
  minha = desenharACamera.minha ?? null;
  const retrato = typeof desenhado !== "undefined" ? desenhado : null;
  const dentro = voice_room != null;

  // A transmissão desta pessoa acabou do lado do servidor (a permissão
  // revogada, a sala que cresceu além da subida) com a câmera ainda acesa.
  //
  // **Só depois de um retrato tê-la mostrado.** O fluxo abre milissegundos
  // depois de o servidor dar nome à transmissão, e o retrato que a janela tem
  // desenhado nesse instante ainda pode ser o de antes: sem esta espera, a
  // câmera que acabou de entrar no ar era apagada pelo retrato atrasado, com a
  // frase de que o servidor a encerrou.
  if (doRetrato && cameraDaJanela.estado === "no-ar") {
    if (minha && minha.tela === cameraDaJanela.tela) cameraDaJanela.noRetrato = true;
    else if (cameraDaJanela.noRetrato) {
      desligarACamera(FRASES_DA_CAMERA.encerrada).catch(() => {});
      return;
    }
  }
  // Fora de sala não há a quem mostrar: a câmera que ficou acesa apaga.
  if (!dentro && cameraLigada()) {
    desligarACamera().catch(() => {});
    return;
  }

  const ligada = cameraLigada();
  // Só o que mudou (`base.js`): este desenho roda a cada retrato.
  porOEscondido(botao, !dentro || cameraSoNoPc());
  porODado(botao, "estado", cameraDaJanela.estado);
  porOAtributo(botao, "aria-pressed", ligada ? "true" : "false");
  const rotulo = ligada ? FRASES_DA_CAMERA.desligar : FRASES_DA_CAMERA.ligar;
  if (botao.getAttribute("aria-label") !== rotulo || !botao.firstElementChild) {
    botao.setAttribute("aria-label", rotulo);
    botao.replaceChildren(glifo("camera"));
  }
  const antigo = dentro && !ligada && !servidorTemACamera(retrato);
  const semJanela = dentro && !ligada && !antigo && !temCameraParaCodificar();
  porOAtributo(botao, "aria-disabled", antigo || semJanela ? "true" : "false");
  porOTitulo(botao, !dentro ? ""
    : ligada ? FRASES_DA_CAMERA.dicaDeDesligar
    : antigo ? FRASES_DA_CAMERA.dicaDoServidorAntigo
    : semJanela ? FRASES_DA_CAMERA.semCodificador.split("\n")[0].toLowerCase()
    : minha ? FRASES_DA_CAMERA.dicaDeLigarNoLugarDaTela
    : FRASES_DA_CAMERA.dicaDeLigar(voice_room.name));

  // Nos dois lugares em que o controle aparece: ao lado do operador, e na
  // bandeja da chamada, onde a coluna do operador sai de cena.
  for (const onde of [estado, $("chamada-camera-estado")]) {
    if (!onde) continue;
    porOEscondido(onde, !dentro || !cameraDaJanela.frase);
    porODado(onde, "estado", cameraDaJanela.estado);
    porOTexto(onde, onde.hidden ? "" : cameraDaJanela.frase);
  }

  desenharAPreviaDaCamera();
}

/** A imagem que quem liga a câmera vê de si, direto da mídia. */
function desenharAPreviaDaCamera() {
  const previa = $("camera-previa");
  if (!previa) return;
  const midia = cameraDaJanela.midia;
  const mostrar = Boolean(midia) && cameraDaJanela.estado !== "pedindo";
  if (mostrar && previa.srcObject !== midia) {
    previa.srcObject = midia;
    previa.play().catch(() => {});
  } else if (!mostrar && previa.srcObject) {
    previa.pause();
    previa.srcObject = null;
  }
  porOEscondido(previa, !mostrar);
  if ($("palco")) porOAtributo($("palco"), "data-camera", mostrar ? "sim" : "nao");
}

$("operador-camera")?.addEventListener("click", () => {
  const acao = cameraLigada() ? desligarACamera() : ligarACamera();
  acao.catch((falha) => {
    console.warn("câmera:", falha);
    escreverEstadoDaCamera("desligada", fraseDeErro(falha));
  });
});
