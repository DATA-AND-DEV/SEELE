// Compartilhar a tela pelo PC, no navegador (Tarefa 3.10 da F4; ADR 0057).
//
// Só na casca larga: no celular a ponte recusa os comandos com a frase
// (`SO_NA_CASCA_LARGA`). Quem captura é o `getDisplayMedia` (o navegador
// mostra o seletor dele, e ali a pessoa escolhe a tela ou a janela), quem
// codifica é o `VideoEncoder`, e o que vai no fio — o tipo do fluxo, a
// abertura, o cabeçalho de cada quadro, as medidas e a cadência pelos degraus
// do desktop — sai do `.wasm` (`seele_web::tela`). Daqui saem só bytes.
//
// # O que o desktop faz, e esta página faz igual
//
// - o pedido fica guardado e aparece no retrato (`minha_transmissao.pedido`);
// - o fluxo abre quando o servidor diz `ScreenShareStarted` para esta pessoa;
// - um quadro-chave sai no começo e a cada `KeyFrameRequested`;
// - o som do que se mostra vai no mesmo fluxo, em quadros de som (R17);
// - quem compartilha vê a própria tela pelos mesmos eventos da casca
//   (`ScreenOpened`, `ScreenFrame`, `ScreenClosed`), como o espelho do desktop;
// - parar é idempotente, e o servidor fica sabendo.
//
// # O que fica dito, e não escondido
//
// O H.264 é o Baseline 4.0 em Annex-B, o que o Chromium e o Safari codificam
// e o OpenH264 do desktop decodifica; a string dele vem do `.wasm`
// (`seele_web::tela::CODEC_DO_NAVEGADOR`), porque quem decide o perfil é quem
// codifica, e a casca não escreve perfil à mão. O `isConfigSupported` do
// Safari diz que codifica e não codifica: o controle só aparece depois de um
// quadro de verdade sair do codificador, num prazo (`sondarATela`). O som da
// tela sai só quando a captura entrega 48 kHz (o Opus do produto); em outra
// taxa, a imagem segue e o console diz por quê.

/** O prazo para a sonda tirar um quadro do codificador. */
const PRAZO_DA_SONDA_MS = 3_000;

/** Quantos quadros podem esperar a escrita antes de a página parar de codificar. */
const FILA_DA_TELA = 8;

/** O quadro de som da tela: 20 ms, o do produto. */
const QUADRO_DO_SOM_DA_TELA_US = 20_000;

let sondaDaTela = null;

/** O leitor nativo de quadros, ou o leitor de vídeo do WebKit. */
function temLeituraDeVideoDaTela() {
  return typeof MediaStreamTrackProcessor === "function" ||
    (typeof HTMLVideoElement === "function" && typeof HTMLVideoElement.prototype.requestVideoFrameCallback === "function");
}
function temCapturaParaCodificar() {
  return Boolean(navigator.mediaDevices?.getDisplayMedia) && typeof VideoEncoder === "function" &&
    typeof VideoFrame === "function" && temLeituraDeVideoDaTela();
}
/** Sem TrackProcessor de áudio, só vídeo; a voz tem seu próprio transporte. */
function temSomDaTelaParaCodificar() {
  return typeof MediaStreamTrackProcessor === "function" && typeof AudioEncoder === "function";
}

/** Um quadro apresentado por vez, sem fila adicional nem outro codificador. */
function leitorDeVideoSemProcessador(trilha) {
  if (typeof HTMLVideoElement.prototype.requestVideoFrameCallback !== "function") throw new Error("Este navegador não lê os quadros da captura de tela.");
  const video = document.createElement("video");
  video.muted = true; video.playsInline = true; video.setAttribute("aria-hidden", "true");
  video.dataset.capturaDaTela = "sim";
  Object.assign(video.style, { position: "fixed", width: "1px", height: "1px", opacity: "0.001", pointerEvents: "none", left: "0", top: "0" });
  video.srcObject = new MediaStream([trilha]); document.body.append(video);
  let fechado = false, callback = null, timer = null, pendente = null, avisarFim, ultimoTempo = -1, ultimoQuadro = null;
  const fim = new Promise(resolve => { avisarFim = resolve; });
  const pronto = video.play(); pronto.catch(() => {});
  function cancelar() {
    if (fechado) return Promise.resolve();
    fechado = true;
    if (callback !== null) video.cancelVideoFrameCallback?.(callback);
    clearTimeout(timer); timer = null;
    callback = null; video.pause(); video.srcObject = null; video.remove();
    pendente?.resolve({ done: true }); pendente = null; avisarFim(false);
    return Promise.resolve();
  }
  return {
    cancel: cancelar,
    async read() {
      if (fechado || !await Promise.race([pronto.then(() => true), fim])) return { done: true };
      if (fechado) return { done: true };
      return new Promise((resolve, reject) => {
        const leitura = { resolve, reject }; pendente = leitura;
        function entregar() {
          if (fechado || pendente !== leitura || video.readyState < 2) return false;
          const tempo = Math.round(video.currentTime * 1000000);
          if (tempo <= ultimoTempo) return false;
          const numero = video.getVideoPlaybackQuality?.().totalVideoFrames;
          if (numero > 0 && numero === ultimoQuadro) return false;
          if (callback !== null) video.cancelVideoFrameCallback?.(callback);
          callback = null; clearTimeout(timer); timer = null;
          pendente = null;
          try {
            const value = new VideoFrame(video, { timestamp: tempo });
            ultimoTempo = tempo; ultimoQuadro = numero; resolve({ done: false, value });
          }
          catch (erro) { void cancelar(); reject(erro); }
          return true;
        }
        function apresentado() {
          callback = null;
          if (!entregar() && !fechado && pendente === leitura) callback = video.requestVideoFrameCallback(apresentado);
        }
        // WKWebView pode deixar de apresentar o vídeo auxiliar quando outra
        // janela o cobre. Ler sua mídia não pode depender só do compositor.
        // Um único prazo por read usa o quadro atual apenas se o tempo da
        // mídia avançou; não cria fila nem substitui a captura já autorizada.
        function verificar() {
          timer = null;
          if (!entregar() && !fechado && pendente === leitura) timer = setTimeout(verificar, 100);
        }
        callback = video.requestVideoFrameCallback(apresentado);
        timer = setTimeout(verificar, 100);
      });
    },
  };
}

/**
 * **Se esta página compartilha a tela**: o `getDisplayMedia`, o
 * `VideoEncoder`, e um quadro de verdade saindo do codificador dentro do
 * prazo, no `codec` do `.wasm`. Feita uma vez por carga.
 */
function sondarATela(codec) {
  if (!sondaDaTela) {
    sondaDaTela = (async () => {
      // A sonda usa a mesma condição do transmissor, incluindo o leitor de
      // vídeo do WebKit quando ele não oferece TrackProcessor.
      if (!temCapturaParaCodificar()) return false;
      const saiu = new Promise((resolver) => {
        let codificador;
        try {
          codificador = new VideoEncoder({ output: () => resolver(true), error: () => resolver(false) });
          codificador.configure({ codec, width: 64, height: 64, bitrate: 100_000, framerate: 8, avc: { format: "annexb" } });
          const tela = new OffscreenCanvas(64, 64);
          tela.getContext("2d").fillRect(0, 0, 64, 64);
          const quadro = new VideoFrame(tela, { timestamp: 0 });
          codificador.encode(quadro, { keyFrame: true });
          quadro.close();
          codificador.flush().catch(() => resolver(false));
        } catch {
          resolver(false);
        }
      });
      const prazo = new Promise((resolver) => setTimeout(() => resolver(false), PRAZO_DA_SONDA_MS));
      return Promise.race([saiu, prazo]);
    })();
  }
  return sondaDaTela;
}

/** Os bytes de um pedaço codificado. */
function bytesDoPedaco(pedaco) {
  const bytes = new Uint8Array(pedaco.byteLength);
  pedaco.copyTo(bytes);
  return bytes;
}

/** Base64 de uns bytes, para o espelho da própria tela (o `data` do `ScreenFrame`). */
function base64DosBytes(bytes) {
  let binario = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binario += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binario);
}

/**
 * A transmissão desta página. `sessao` é a da ponte (`atender`, `abrirUmFluxo`),
 * `wasm` as exportações da página (o `tela_quadro`), `eu()` a pessoa desta
 * sessão, e `emitir(evento)` o canal da casca.
 */
function criarATransmissaoDoNavegador({ sessao, wasm, eu, emitir }) {
  const memoria = () => new Uint8Array(wasm.memory.buffer);
  const cabecalhoDoQuadro = (tipo, tamanho) =>
    wasm.tela_quadro(tipo, tamanho) === 0 ? memoria().slice(wasm.saida_ptr(), wasm.saida_ptr() + wasm.saida_len()) : null;

  /** O que está no ar: a mídia, os limites, e o fluxo depois do `ScreenShareStarted`. */
  let viva = null;

  function pedir(comando, args) {
    const feito = sessao.atender(comando, args);
    if (!feito || "erro" in feito) throw feito?.erro ?? "NotConnected";
    return feito.resposta;
  }

  /** Começa: a mídia que o seletor deu, e o pedido ao servidor. */
  function comecar(midia, limites) {
    if (viva) fechar({ avisar: false });
    viva = { midia, limites, tela: null, fluxo: null };
    for (const trilha of midia.getTracks()) {
      // O PARAR do próprio navegador: a trilha acaba, e a transmissão também.
      trilha.addEventListener("ended", () => parar());
    }
    pedir("compartilhar_tela", { limites });
  }

  /** As mensagens da sessão que mexem na transmissão desta página. */
  function aoDobrar(mensagem) {
    if (!viva || !mensagem) return;
    const comecou = mensagem.ScreenShareStarted;
    if (comecou && comecou.person === eu() && viva.tela === null) {
      viva.tela = comecou.screen;
      abrir().catch((falha) => {
        console.warn("tela: o fluxo da transmissão não abriu:", falha);
        parar();
      });
    }
    const chave = mensagem.KeyFrameRequested;
    if (chave && chave.screen === viva.tela && viva.fluxo) viva.fluxo.chave = true;
    const parou = mensagem.ScreenShareStopped;
    if (parou && parou.screen === viva.tela) fechar({ avisar: false });
  }

  /** O fluxo da tela: a abertura, e os quadros da imagem e do som. */
  async function abrir() {
    const estaVez = viva;
    const trilha = estaVez.midia.getVideoTracks()[0];
    const ajustes = trilha.getSettings();
    const medidas = pedir("codificacao_da_tela", {
      limites: estaVez.limites,
      largura: ajustes.width ?? 0,
      altura: ajustes.height ?? 0,
    });
    const abertura = pedir("abertura_da_tela", {
      tela: estaVez.tela,
      monitor: ajustes.displaySurface !== "window",
      largura: medidas.largura,
      altura: medidas.altura,
    });
    const escritor = (await sessao.abrirUmFluxo()).getWriter();
    if (viva !== estaVez) {
      escritor.close().catch(() => {});
      return;
    }
    const fluxo = { escritor, escrita: Promise.resolve(), naFila: 0, chave: true, medidas, codificadores: [], leitores: [] };
    estaVez.fluxo = fluxo;
    escrever(fluxo, new Uint8Array(abertura));
    emitir({ ScreenOpened: { screen: estaVez.tela, width: medidas.largura, height: medidas.altura } });

    // A imagem.
    fluxo.configuracaoVideo = {
      codec: medidas.codec,
      width: medidas.largura,
      height: medidas.altura,
      bitrate: medidas.banda_bps,
      framerate: medidas.quadros,
      latencyMode: "realtime",
      avc: { format: "annexb" },
    };
    fluxo.recuperacoesVideo = 0;
    criarCodificadorDaImagem(estaVez, fluxo);
    lerAImagem(estaVez, trilha);

    // O som, no mesmo fluxo.
    const som = estaVez.midia.getAudioTracks()[0];
    if (som && estaVez.limites.com_som !== false && temSomDaTelaParaCodificar()) lerOSom(estaVez, som);
    else if (som && !temSomDaTelaParaCodificar()) som.stop();
  }

  function criarCodificadorDaImagem(estaVez, fluxo) {
    const codificador = new VideoEncoder({
      output: pedaco => {
        if (fluxo.codificadorVideo === codificador) quadro(estaVez, pedaco.type === "key" ? 1 : 0, bytesDoPedaco(pedaco));
      },
      error: falha => recuperarCodificadorDaImagem(estaVez, fluxo, codificador, falha),
    });
    fluxo.codificadorVideo = codificador;
    fluxo.codificadores.push(codificador);
    fluxo.chave = true;
    codificador.configure(fluxo.configuracaoVideo);
  }

  function recuperarCodificadorDaImagem(estaVez, fluxo, codificador, falha) {
    if (viva !== estaVez || estaVez.fluxo !== fluxo || fluxo.codificadorVideo !== codificador) return;
    if (codificador.state !== "closed") codificador.close();
    fluxo.codificadores = fluxo.codificadores.filter(item => item !== codificador);
    // O navegador pode recolher recursos do codec. Reutilizar um encoder
    // fechado deixa a UI no ar, mas sem imagem. A captura e o fluxo seguem;
    // apenas o codec é substituído, com chave, por no máximo duas tentativas.
    if (["QuotaExceededError", "EncodingError"].includes(falha.name) && fluxo.recuperacoesVideo++ < 2) {
      try { criarCodificadorDaImagem(estaVez, fluxo); return; }
      catch (erro) { console.warn("tela: não foi possível recuperar o codificador:", erro); }
    }
    console.warn("tela: o codificador da imagem parou:", falha);
    parar("A imagem compartilhada parou. Compartilhe a tela novamente.");
  }

  function escrever(fluxo, bytes) {
    fluxo.naFila += 1;
    fluxo.escrita = fluxo.escrita
      .then(() => fluxo.escritor.write(bytes))
      .catch((falha) => {
        if (!fluxo.caiu) console.warn("tela: a escrita do fluxo falhou:", falha);
        fluxo.caiu = true;
      })
      .finally(() => {
        fluxo.naFila -= 1;
      });
  }

  /** Um quadro codificado: o cabeçalho do `.wasm` e os bytes; e o espelho, se for imagem. */
  function quadro(estaVez, tipo, bytes) {
    const fluxo = estaVez.fluxo;
    if (!fluxo || viva !== estaVez) return;
    const cabecalho = cabecalhoDoQuadro(tipo, bytes.length);
    if (!cabecalho) return;
    const junto = new Uint8Array(cabecalho.length + bytes.length);
    junto.set(cabecalho);
    junto.set(bytes, cabecalho.length);
    escrever(fluxo, junto);
    if (tipo !== 2) emitir({ ScreenFrame: { screen: estaVez.tela, key: tipo === 1, data: base64DosBytes(bytes) } });
  }

  /** A imagem da captura, na cadência e nas medidas do degrau. */
  async function lerAImagem(estaVez, trilha) {
    const fluxo = estaVez.fluxo;
    const { largura, altura, quadros } = fluxo.medidas;
    const leitor = typeof MediaStreamTrackProcessor === "function"
      ? new MediaStreamTrackProcessor({ track: trilha }).readable.getReader()
      : leitorDeVideoSemProcessador(trilha);
    fluxo.leitores.push(leitor);
    const tela = new OffscreenCanvas(largura, altura);
    const contexto = tela.getContext("2d");
    const intervaloUs = 1_000_000 / quadros;
    let ultimo = -Infinity;
    for (;;) {
      let lido;
      try {
        lido = await leitor.read();
      } catch (erro) {
        console.warn("tela: a leitura da captura parou:", erro);
        if (viva === estaVez && estaVez.fluxo === fluxo) parar("Não foi possível continuar lendo a captura. Compartilhe a tela novamente.");
        break;
      }
      if (lido.done || viva !== estaVez || estaVez.fluxo !== fluxo) {
        lido.value?.close();
        if (lido.done && viva === estaVez && estaVez.fluxo === fluxo) parar("A captura da imagem terminou. Compartilhe a tela novamente.");
        break;
      }
      const original = lido.value;
      const codificador = fluxo.codificadorVideo;
      const agora = original.timestamp;
      // A cadência pedida, e a fila de escrita: um quadro que não cabe agora
      // é descartado, e o próximo sai como chave.
      if (agora - ultimo < intervaloUs || fluxo.naFila > FILA_DA_TELA || codificador.encodeQueueSize > 2) {
        if (fluxo.naFila > FILA_DA_TELA) fluxo.chave = true;
        original.close();
        continue;
      }
      ultimo = agora;
      // A janela pode mudar de tamanho depois do ScreenOpened. O quadro
      // efetivo manda na proporção; esticá-lo até as medidas iniciais achata
      // monitores verticais e janelas redimensionadas durante a transmissão.
      const fator = Math.min(largura / original.displayWidth, altura / original.displayHeight);
      const w = original.displayWidth * fator, h = original.displayHeight * fator;
      contexto.fillStyle = "black";
      contexto.fillRect(0, 0, largura, altura);
      contexto.drawImage(original, (largura - w) / 2, (altura - h) / 2, w, h);
      original.close();
      const quadroNovo = new VideoFrame(tela, { timestamp: agora });
      try {
        codificador.encode(quadroNovo, { keyFrame: fluxo.chave });
        fluxo.chave = false;
      } catch (falha) {
        recuperarCodificadorDaImagem(estaVez, fluxo, codificador, falha);
      } finally {
        quadroNovo.close();
      }
    }
  }

  /** O som da captura, em Opus de 20 ms, quando ela entrega 48 kHz. */
  function lerOSom(estaVez, som) {
    const fluxo = estaVez.fluxo;
    const taxa = som.getSettings().sampleRate;
    if (taxa && taxa !== 48_000) {
      console.warn(`tela: a captura entrega o som a ${taxa} Hz, e o som da tela sai só a 48 kHz: a imagem segue sem som`);
      return;
    }
    let codificador = null;
    const leitor = new MediaStreamTrackProcessor({ track: som }).readable.getReader();
    fluxo.leitores.push(leitor);
    (async () => {
      for (;;) {
        let lido;
        try {
          lido = await leitor.read();
        } catch {
          break;
        }
        if (lido.done || viva !== estaVez) {
          lido.value?.close();
          break;
        }
        const dados = lido.value;
        if (!codificador) {
          codificador = new AudioEncoder({
            output: (pedaco) => quadro(estaVez, 2, bytesDoPedaco(pedaco)),
            error: (falha) => console.warn("tela: o codificador do som falhou:", falha),
          });
          codificador.configure({
            codec: "opus",
            sampleRate: dados.sampleRate,
            numberOfChannels: dados.numberOfChannels,
            bitrate: 64_000,
            opus: { frameDuration: QUADRO_DO_SOM_DA_TELA_US },
          });
          fluxo.codificadores.push(codificador);
        }
        try {
          codificador.encode(dados);
        } catch (falha) {
          console.warn("tela: um pedaço de som não entrou no codificador:", falha);
        } finally {
          dados.close();
        }
      }
    })();
  }

  /** Fecha o fluxo e os codificadores; `avisar` diz à casca que a tela fechou. */
  function fechar({ avisar = true, erro = null } = {}) {
    const estaVez = viva;
    viva = null;
    if (!estaVez) return;
    const fluxo = estaVez.fluxo;
    if (fluxo) {
      for (const leitor of fluxo.leitores) leitor.cancel().catch(() => {});
      for (const codificador of fluxo.codificadores) {
        if (codificador.state !== "closed") codificador.close();
      }
      fluxo.escrita.finally(() => fluxo.escritor.close().catch(() => {}));
    }
    for (const trilha of estaVez.midia.getTracks()) trilha.stop();
    if (estaVez.tela !== null && (avisar || fluxo)) emitir({ ScreenClosed: { screen: estaVez.tela, ...(erro ? { erro } : {}) } });
  }

  /** Para: o fluxo fecha, a captura solta, e o servidor fica sabendo. Idempotente. */
  function parar(erro = null) {
    fechar({ erro });
    try {
      pedir("parar_de_compartilhar", {});
    } catch {
      // Sem sessão: não há a quem avisar.
    }
  }

  /**
   * Os limites novos: guardados (o retrato os mostra), e, no ar, a banda muda
   * na hora e a medida ou a cadência recomeçam o fluxo com a abertura nova,
   * como no desktop.
   */
  function ajustar(limites) {
    pedir("ajustar_limites_da_tela", { limites });
    if (!viva) return;
    const antes = viva.limites;
    viva.limites = limites;
    const fluxo = viva.fluxo;
    if (!fluxo) return;
    const mudouAMedida = antes.altura_maxima !== limites.altura_maxima || antes.quadros_maximos !== limites.quadros_maximos;
    if (mudouAMedida) {
      const estaVez = viva;
      for (const leitor of fluxo.leitores) leitor.cancel().catch(() => {});
      for (const codificador of fluxo.codificadores) if (codificador.state !== "closed") codificador.close();
      fluxo.escrita.finally(() => fluxo.escritor.close().catch(() => {}));
      estaVez.fluxo = null;
      abrir().catch((falha) => console.warn("tela: o fluxo não reabriu com os limites novos:", falha));
      return;
    }
    const imagem = fluxo.codificadores.find((codificador) => codificador instanceof VideoEncoder);
    if (imagem && imagem.state === "configured" && limites.banda_bps) {
      fluxo.configuracaoVideo = { ...fluxo.configuracaoVideo, bitrate: limites.banda_bps };
      imagem.configure(fluxo.configuracaoVideo);
      fluxo.chave = true;
    }
  }

  return {
    comecar,
    aoDobrar,
    parar,
    ajustar,
    fechar,
    get noAr() {
      return viva !== null;
    },
  };
}
