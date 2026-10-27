// Tela pessoal: o mesmo transmissor/LeitorDeTela SEELE, sem Snapshot de servidor.
// Abrir/Dados/Fechar viajam no MESMO canal ordenado. Os avisos do controle não
// abrem nem fecham leitores: não existe ordem entre dois DataChannels.
function criarCompartilhamentoPessoalWeb({ servico, transporte, palco, enviarControle, aoEstado = () => {} }) {
  const wasm = servico.wasm;
  let fechado = false, captura = 0, geracaoLocal = 0, geracaoRemota = 0;
  let videoNativo = null, prazoVideo = null, cancelarCaptura = null;
  let leitura = null, decoder = null, esperaChave = true, timestamp = 0, ultimoPedido = -Infinity;
  const canvas = document.createElement("canvas");
  canvas.setAttribute("aria-label", "Imagem compartilhada pela outra pessoa");
  canvas.style.width = "100%"; canvas.style.maxHeight = "70vh"; canvas.style.objectFit = "contain";
  const expandir = document.createElement("button");
  expandir.type = "button"; expandir.textContent = "TELA CHEIA";
  palco.append(canvas, expandir);
  palco.hidden = true;
  function estado(tipo, detalhe) { if (!fechado) aoEstado(tipo, detalhe); }
  function controle(mensagem) { Promise.resolve(enviarControle(mensagem)).catch(erro => estado("erro", String(erro))); }
  const jsonDaSaida = () => JSON.parse(new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, wasm.saida_ptr(), wasm.saida_len()).slice()));
  function passos(alca, bytes) {
    const p = wasm.alocar(bytes.length);
    let codigo;
    try { new Uint8Array(wasm.memory.buffer).set(bytes, p); codigo = wasm.fluxo_chegou(alca, p, bytes.length); }
    finally { wasm.liberar(p, bytes.length); }
    if (codigo !== 0) throw new Error("Não foi possível ler a tela compartilhada.");
    return jsonDaSaida().passos;
  }
  function soltarDecoder() { if (decoder && decoder.state !== "closed") decoder.close(); decoder = null; esperaChave = true; }
  function fecharRecepcao() {
    soltarVideoNativo();
    const doc = palco.ownerDocument;
    if (doc.fullscreenElement === palco || doc.webkitFullscreenElement === palco) Promise.resolve(doc.exitFullscreen?.() ?? doc.webkitExitFullscreen?.()).catch(() => {});
    const anterior = leitura; leitura = null;
    if (anterior) wasm.fluxo_acabou(anterior.alca);
    soltarDecoder(); palco.hidden = true;
  }
  function pedirChave() {
    if (performance.now() - ultimoPedido < 500) return;
    ultimoPedido = performance.now(); controle("PedirChave");
  }
  function quadro(corpo, identidade) {
    if (leitura !== identidade || fechado) return;
    if (!corpo.key && esperaChave) return;
    const bytes = Uint8Array.from(atob(corpo.data), c => c.charCodeAt(0));
    if (!decoder) {
      if (!corpo.key) return;
      if (typeof VideoDecoder !== "function") throw new Error("Este navegador não decodifica a tela compartilhada.");
      const codec = codecDoSps(bytes);
      if (!codec) throw new Error("A tela não enviou um quadro-chave válido.");
      const este = new VideoDecoder({
        output(frame) {
          try {
            if (fechado || leitura !== identidade || decoder !== este) return;
            canvas.getContext("2d").drawImage(frame, 0, 0, canvas.width, canvas.height);
            estado("quadro", { largura: canvas.width, altura: canvas.height, geracao: identidade.geracao });
          } finally { frame.close(); }
        },
        error(erro) {
          if (leitura !== identidade || decoder !== este || fechado) return;
          soltarDecoder(); pedirChave(); estado("erro", String(erro));
        },
      });
      decoder = este;
      decoder.configure({ codec, codedWidth: canvas.width, codedHeight: canvas.height, optimizeForLatency: true });
    }
    if (decoder.decodeQueueSize > 3) { esperaChave = true; pedirChave(); return; }
    if (corpo.key) esperaChave = false;
    decoder.decode(new EncodedVideoChunk({ type: corpo.key ? "key" : "delta", timestamp: timestamp++ * 33333, data: bytes }));
  }
  function receber(bytes) {
    if (fechado) return;
    try {
      const mensagem = servico.operar({ acao: "tela_desempacotar", bytes: Array.from(bytes) }).resposta;
      if (mensagem.Abrir) {
        const geracao = mensagem.Abrir.geracao;
        if (geracao <= geracaoRemota) return;
        fecharRecepcao(); geracaoRemota = geracao;
        leitura = { geracao, alca: wasm.fluxo_novo() }; timestamp = 0;
      } else if (mensagem.Dados) {
        if (!leitura || mensagem.Dados.geracao !== leitura.geracao) return;
        const identidade = leitura;
        for (const passo of passos(identidade.alca, new Uint8Array(mensagem.Dados.bytes))) {
          const evento = passo.evento;
          if (evento?.ScreenOpened) {
            const { width, height } = evento.ScreenOpened;
            if (!width || !height || width * height > 16777216) throw new Error("Medidas da tela fora do limite.");
            canvas.width = width; canvas.height = height; palco.hidden = false; estado("recebendo", { largura: width, altura: height });
          } else if (evento?.ScreenFrame) quadro(evento.ScreenFrame, identidade);
          else if (evento?.ScreenUnreadable || passo.ignorado) throw new Error("O fluxo da tela não pôde ser lido.");
          else if (evento?.ScreenClosed) fecharRecepcao();
          // A captura pessoal é vídeo. Não se pede áudio de sistema; voz usa
          // o seu canal próprio e não se mistura com som da tela sem controle.
        }
      } else if (mensagem.Fechar && leitura?.geracao === mensagem.Fechar.geracao) {
        fecharRecepcao(); estado("tela-parou");
      }
    } catch (erro) { fecharRecepcao(); estado("erro", String(erro)); }
  }
  const desouvir = transporte.ouvir(receber);
  function empacotar(mensagem) {
    return new Uint8Array(servico.operar({ acao: "tela_empacotar", mensagem }).bytes);
  }
  function abrirEscrita() {
    if (fechado || geracaoLocal >= 0xffffffff) throw new Error("Compartilhamento encerrado.");
    const geracao = ++geracaoLocal;
    return new WritableStream({
      start() { return transporte.enviar(empacotar({ Abrir: { geracao } })); },
      async write(bytes) {
        for (let inicio = 0; inicio < bytes.length; inicio += 16000) {
          if (fechado || geracao !== geracaoLocal) return;
          await transporte.enviar(empacotar({ Dados: { geracao, bytes: Array.from(bytes.subarray(inicio, inicio + 16000)) } }));
        }
      },
      close() { if (!fechado) return transporte.enviar(empacotar({ Fechar: { geracao } })); },
    });
  }
  // Esta pequena adaptação só traduz os verbos do transmissor reutilizado
  // para operações puras da chamada. Não inventa identidade/autorização de
  // servidor, nem publica eventos na sessão do aplicativo.
  let transmissao;
  const comandos = {
    atender(comando, args) {
      if (fechado) return { erro: "encerrada" };
      if (comando === "codificacao_da_tela") return servico.operar({ acao: comando, ...args });
      if (comando === "abertura_da_tela") return { resposta: servico.operar({ acao: comando, largura: args.largura, altura: args.altura }).bytes };
      if (comando === "compartilhar_tela") {
        queueMicrotask(() => { if (!fechado) transmissao.aoDobrar({ ScreenShareStarted: { person: 1, screen: 1 } }); });
      }
      return { resposta: null };
    },
    abrirUmFluxo: async () => abrirEscrita(),
  };
  transmissao = criarATransmissaoDoNavegador({ sessao: comandos, wasm, eu: () => 1, emitir(evento) {
    if (evento.ScreenOpened) {
      const { width, height } = evento.ScreenOpened;
      controle({ TelaComecou: { largura: width, altura: height } }); estado("transmitindo");
    } else if (evento.ScreenClosed) { controle("TelaParou"); estado("parada"); }
  } });
  function parar() { captura++; cancelarCaptura?.(); transmissao.parar(); }
  function usarMidia(midia, limites = { altura_maxima: 720, quadros_maximos: 30, banda_bps: 1200000, com_som: false }) {
    if (fechado) { midia.getTracks().forEach(t => t.stop()); throw new Error("Conversa encerrada."); }
    if (!midia.getVideoTracks().length) { midia.getTracks().forEach(t => t.stop()); throw new Error("A captura não tem imagem."); }
    // O adaptador não anuncia suporte ao som de sistema sem um reprodutor.
    for (const t of midia.getAudioTracks()) { t.stop(); midia.removeTrack(t); }
    try { transmissao.comecar(midia, { ...limites, com_som: false }); }
    catch (erro) { midia.getTracks().forEach(t => t.stop()); throw erro; }
  }
  async function compartilhar() {
    if (fechado || !temCapturaParaCodificar()) throw new Error("Este navegador não compartilha tela. Use o aplicativo de computador.");
    cancelarCaptura?.();
    const vez = ++captura;
    const obtendo = navigator.mediaDevices.getDisplayMedia({ video: true, audio: false });
    // O seletor do navegador não pode ser fechado pela página. Cancelar solta
    // a operação da UI agora; se a escolha vier depois, suas faixas são paradas.
    obtendo.then(midia => { if (fechado || vez !== captura) midia.getTracks().forEach(t => t.stop()); }, () => {});
    let cancelar, timer;
    const cancelada = new Promise(ok => { cancelar = () => ok(null); });
    cancelarCaptura = cancelar;
    const prazo = new Promise((_, nao) => { timer = setTimeout(() => { if (vez === captura) captura++; nao(new Error("A seleção de tela não terminou. Tente novamente.")); }, 30000); });
    let midia;
    try { midia = await Promise.race([obtendo, cancelada, prazo]); }
    finally { clearTimeout(timer); if (cancelarCaptura === cancelar) cancelarCaptura = null; }
    if (!midia) return;
    if (fechado || vez !== captura) { midia.getTracks().forEach(t => t.stop()); return; }
    usarMidia(midia);
  }
  function receberControle(mensagem) {
    if (mensagem === "PedirChave") transmissao.aoDobrar({ KeyFrameRequested: { screen: 1 } });
    // TelaComecou/Parou não tocam leitura/decoder. A ordem válida é do fluxo.
  }
  function soltarVideoNativo() {
    clearTimeout(prazoVideo);
    const video = videoNativo; videoNativo = null;
    if (!video) return;
    try { if (video.webkitDisplayingFullscreen) video.webkitExitFullscreen(); } catch { /* já saiu */ }
    for (const t of video.srcObject?.getTracks() ?? []) t.stop();
    video.pause(); video.srcObject = null; video.remove(); expandir.textContent = "TELA CHEIA";
  }
  function prepararVideoNativo() {
    const video = document.createElement("video");
    if (!video.webkitEnterFullscreen || !canvas.captureStream) return false;
    video.muted = true; video.playsInline = true; video.controls = true;
    Object.assign(video.style, { position: "fixed", width: "1px", height: "1px", bottom: "0", left: "0", opacity: "0.001", pointerEvents: "none" });
    video.srcObject = canvas.captureStream(15); videoNativo = video; palco.append(video);
    video.addEventListener("webkitendfullscreen", soltarVideoNativo, { once: true });
    video.addEventListener("loadedmetadata", () => { if (videoNativo === video) { clearTimeout(prazoVideo); expandir.textContent = "ABRIR TELA CHEIA"; } });
    prazoVideo = setTimeout(soltarVideoNativo, 3000);
    video.play().catch(() => { if (videoNativo === video) soltarVideoNativo(); });
    return true;
  }
  async function telaCheia() {
    const doc = palco.ownerDocument;
    if (doc.fullscreenElement === palco || doc.webkitFullscreenElement === palco) {
      await (doc.exitFullscreen?.() ?? doc.webkitExitFullscreen?.()); return;
    }
    if (videoNativo?.readyState >= 1) { videoNativo.webkitEnterFullscreen(); return; }
    const pedir = palco.requestFullscreen ?? palco.webkitRequestFullscreen;
    if (pedir) { try { await pedir.call(palco); return; } catch { /* fallback de vídeo do Safari */ } }
    // Preparar o vídeo pode consumir o gesto: o segundo toque é explícito.
    if (!videoNativo && !prepararVideoNativo()) estado("erro", "Tela cheia indisponível neste navegador.");
  }
  const atualizarBotao = () => { expandir.textContent = (document.fullscreenElement === palco || document.webkitFullscreenElement === palco) ? "SAIR DA TELA CHEIA" : "TELA CHEIA"; };
  expandir.addEventListener("click", () => telaCheia().catch(erro => estado("erro", String(erro))));
  document.addEventListener("fullscreenchange", atualizarBotao); document.addEventListener("webkitfullscreenchange", atualizarBotao);
  function fechar() {
    if (fechado) return;
    parar(); fechado = true; desouvir(); fecharRecepcao();
    document.removeEventListener("fullscreenchange", atualizarBotao); document.removeEventListener("webkitfullscreenchange", atualizarBotao);
    if (document.fullscreenElement === palco || document.webkitFullscreenElement === palco) Promise.resolve(document.exitFullscreen?.() ?? document.webkitExitFullscreen?.()).catch(() => {});
    canvas.remove(); expandir.remove();
  }
  return { compartilhar, usarMidia, parar, receberControle, telaCheia, fechar, get noAr() { return transmissao.noAr; } };
}
