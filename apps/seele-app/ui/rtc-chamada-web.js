// Transporte de chamada entre navegadores. A fachada autenticada confere a
// prova do protocolo em WASM ANTES de entregar receberSinalAutenticado.
// Não descobre pares, não busca SDP e usa apenas STUN. Áudio/tela são bytes
// SEELE opacos: os adaptadores de mídia continuam fora deste módulo.
function criarEnlaceDaChamadaWeb({
  papel, sinal, enviarSinal, aoEstado = () => {}, iceServers = [],
}) {
  if (!["quem-liga", "quem-recebe"].includes(papel) || typeof enviarSinal !== "function") {
    throw new TypeError("chamada: papel ou sinalização inválidos");
  }
  for (const servidor of iceServers) {
    const urls = Array.isArray(servidor.urls) ? servidor.urls : [servidor.urls];
    if (!urls.length || urls.some(url => typeof url !== "string" || !/^stuns?:[^\s]+$/i.test(url)) || Object.keys(servidor).some(chave => chave !== "urls")) {
      throw new TypeError("chamada: somente STUN sem credenciais");
    }
  }
  const pc = new RTCPeerConnection({ iceServers, bundlePolicy: "max-bundle" });
  const canais = {
    controle: pc.createDataChannel("ctl", { negotiated: true, id: 0, ordered: true }),
    voz: pc.createDataChannel("voz", { negotiated: true, id: 1, ordered: false, maxRetransmits: 0 }),
    tela: pc.createDataChannel("tela", { negotiated: true, id: 2, ordered: true }),
  };
  // Perfil opcional, sem negociação DCEP e fora da condição de abertura dos
  // três canais: pares da 0.18 mantêm o transporte que já conhecem.
  const canalDoPerfil = pc.createDataChannel("perfil", { negotiated: true, id: 3, ordered: true });
  const MAX_MENSAGEM = 16 * 1024;
  const MAX_VOZ = 1100;
  const MAX_FILA = 1024 * 1024;
  const LIMIAR = 64 * 1024;
  let estado = "novo", iniciado = false, terminou = false, abriu = false;
  let sinaisEmOrdem = Promise.resolve();
  const limpezas = new Set();
  const contas = { vozDescartada: 0, esperasPorPressao: 0 };
  let resolverPronto, recusarPronto, recusarEncerrado;
  const pronto = new Promise((ok, nao) => { resolverPronto = ok; recusarPronto = nao; });
  const encerrado = new Promise((_, nao) => { recusarEncerrado = nao; });
  // São promessas públicas/competidoras; fechar antes de o consumidor esperar
  // não deixa rejeição sem observador. Quem as espera ainda recebe a falha.
  pronto.catch(() => {}); encerrado.catch(() => {});
  const prazo = setTimeout(() => fechar(new Error("chamada: prazo de conexão esgotado")), 10000);
  function mudar(proximo) { estado = proximo; try { aoEstado(proximo); } catch { /* observador não é dono do transporte */ } }
  function fechar(motivo = new Error("chamada: encerrada")) {
    if (terminou) return;
    terminou = true;
    clearTimeout(prazo);
    sinal?.removeEventListener("abort", abortar);
    for (const limpar of [...limpezas]) limpar(motivo);
    limpezas.clear();
    for (const canal of Object.values(canais)) canal.close();
    pc.close();
    recusarPronto(motivo); recusarEncerrado(motivo);
    mudar("encerrada");
  }
  function abortar() { fechar(new Error("chamada: cancelada")); }
  function conferirViva() { if (terminou) throw new Error("chamada: encerrada"); }
  async function esperar(promessa) { const valor = await Promise.race([promessa, encerrado]); conferirViva(); return valor; }
  function conferirAbertura() {
    if (terminou || abriu || !Object.values(canais).every(c => c.readyState === "open")) return;
    abriu = true; clearTimeout(prazo); mudar("aberta"); resolverPronto();
  }
  pc.addEventListener("connectionstatechange", () => {
    if (["failed", "closed", "disconnected"].includes(pc.connectionState)) fechar(new Error("chamada: conexão interrompida"));
  });
  pc.addEventListener("iceconnectionstatechange", () => {
    if (["failed", "closed"].includes(pc.iceConnectionState)) fechar(new Error("chamada: caminho direto indisponível"));
  });
  pc.addEventListener("datachannel", evento => { evento.channel.close(); fechar(new Error("chamada: canal não negociado")); });

  function adaptar(canal, voz) {
    canal.binaryType = "arraybuffer";
    canal.bufferedAmountLowThreshold = LIMIAR / 2;
    const ouvintes = new Set(), recebidos = [];
    let bytesRecebidos = 0, bytesPendentes = 0, pedidosPendentes = 0, fila = Promise.resolve();
    const teto = voz ? MAX_VOZ : MAX_MENSAGEM;
    function limpar() { recebidos.length = 0; bytesRecebidos = 0; ouvintes.clear(); }
    limpezas.add(limpar);
    canal.addEventListener("open", conferirAbertura);
    canal.addEventListener("close", () => fechar(new Error("chamada: canal encerrado")));
    canal.addEventListener("error", () => fechar(new Error("chamada: erro no canal")));
    canal.addEventListener("message", evento => {
      if (terminou) return;
      if (!(evento.data instanceof ArrayBuffer) || evento.data.byteLength > teto) {
        fechar(new Error("chamada: mensagem inválida")); return;
      }
      const bytes = new Uint8Array(evento.data);
      if (ouvintes.size) { for (const ouvir of ouvintes) ouvir(bytes); return; }
      // Voz é tempo real: iniciar o receptor não deve reproduzir o passado.
      if (voz) { contas.vozDescartada++; return; }
      if (recebidos.length >= 256 || bytesRecebidos + bytes.byteLength > (voz ? LIMIAR : MAX_FILA)) {
        if (voz) { contas.vozDescartada++; return; }
        fechar(new Error("chamada: fila de recepção cheia")); return;
      }
      recebidos.push(bytes); bytesRecebidos += bytes.byteLength;
    });
    function pressao() {
      if (canal.bufferedAmount < LIMIAR) return Promise.resolve();
      contas.esperasPorPressao++;
      return new Promise((ok, nao) => {
        let timer;
        const terminar = erro => {
          clearTimeout(timer); canal.removeEventListener("bufferedamountlow", andar); limpezas.delete(terminar);
          erro ? nao(erro) : ok();
        };
        const andar = () => { if (canal.bufferedAmount < LIMIAR) terminar(); };
        limpezas.add(terminar); canal.addEventListener("bufferedamountlow", andar);
        timer = setTimeout(() => { const erro = new Error("chamada: envio sem progresso"); terminar(erro); fechar(erro); }, 5000);
        andar();
      });
    }
    return {
      // Controle/tela devolvem Promise<boolean>; voz devolve boolean SÍNCRONO,
      // como criarVozDoNavegador espera, e nunca espera uma fila de SCTP.
      enviar(bytes) {
        const valido = bytes instanceof Uint8Array && bytes.byteLength <= teto;
        if (voz) {
          if (!valido || terminou || canal.readyState !== "open" || canal.bufferedAmount >= LIMIAR) { contas.vozDescartada++; return false; }
          try { canal.send(bytes); return true; }
          catch (erro) { fechar(erro); return false; }
        }
        if (!valido) return Promise.reject(new TypeError("chamada: tamanho ou tipo inválido"));
        if (terminou) return Promise.reject(new Error("chamada: encerrada"));
        if (pedidosPendentes >= 256 || bytesPendentes + bytes.byteLength > MAX_FILA) return Promise.reject(new Error("chamada: fila de envio cheia"));
        pedidosPendentes++;
        const copia = bytes.slice(); bytesPendentes += copia.byteLength;
        const pedido = fila.then(async () => { await esperar(pronto); await esperar(pressao()); canal.send(copia); return true; });
        fila = pedido.catch(erro => { fechar(erro); });
        return pedido.finally(() => { bytesPendentes -= copia.byteLength; pedidosPendentes--; });
      },
      ouvir(ouvir) {
        if (typeof ouvir !== "function") throw new TypeError("chamada: ouvinte inválido");
        if (terminou) return () => {};
        ouvintes.add(ouvir);
        for (const bytes of recebidos.splice(0)) ouvir(bytes);
        bytesRecebidos = 0;
        return () => ouvintes.delete(ouvir);
      },
    };
  }
  const controle = adaptar(canais.controle, false);
  const transporteDaVoz = adaptar(canais.voz, true);
  const tela = adaptar(canais.tela, false);
  // A moldura (cabeçalho/tamanho/codec) pertence ao adaptador SEELE. Este
  // WritableStream preserva seus bytes e apenas reparte mensagens SCTP.
  tela.criarEscrita = () => new WritableStream({
    async write(bytes) {
      if (!(bytes instanceof Uint8Array)) throw new TypeError("chamada: tela espera bytes");
      for (let offset = 0; offset < bytes.byteLength; offset += MAX_MENSAGEM) {
        await tela.enviar(bytes.subarray(offset, offset + MAX_MENSAGEM));
      }
    },
  });

  let perfilAEnviar = null;
  const ouvintesDoPerfil = new Set();
  const perfisPendentes = [];
  canalDoPerfil.binaryType = "arraybuffer";
  canalDoPerfil.addEventListener("message", evento => {
    if (terminou || !(evento.data instanceof ArrayBuffer) || evento.data.byteLength > MAX_MENSAGEM) return;
    const bytes = new Uint8Array(evento.data);
    if (ouvintesDoPerfil.size) for (const ouvir of ouvintesDoPerfil) ouvir(bytes);
    else if (perfisPendentes.length < 1) perfisPendentes.push(bytes);
  });
  function enviarPerfilPendente() {
    if (!perfilAEnviar || terminou || canalDoPerfil.readyState !== "open") return;
    const bytes = perfilAEnviar; perfilAEnviar = null;
    try { if (canalDoPerfil.bufferedAmount <= LIMIAR) canalDoPerfil.send(bytes); } catch { /* O canal é opcional. */ }
  }
  canalDoPerfil.addEventListener("open", enviarPerfilPendente);
  limpezas.add(() => { perfilAEnviar = null; ouvintesDoPerfil.clear(); perfisPendentes.length = 0; canalDoPerfil.close(); });
  const perfil = {
    enviar(bytes) {
      if (terminou || !["connecting", "open"].includes(canalDoPerfil.readyState) || perfilAEnviar || !(bytes instanceof Uint8Array) || bytes.byteLength > MAX_MENSAGEM || canalDoPerfil.bufferedAmount > LIMIAR) return false;
      perfilAEnviar = bytes.slice(); enviarPerfilPendente(); return true;
    },
    ouvir(ouvir) {
      if (terminou) return () => {};
      ouvintesDoPerfil.add(ouvir);
      for (const bytes of perfisPendentes.splice(0)) ouvir(bytes);
      return () => ouvintesDoPerfil.delete(ouvir);
    },
  };

  async function colherCandidatos() {
    if (pc.iceGatheringState === "complete") return;
    await esperar(new Promise((ok, nao) => {
      const limpar = erro => { pc.removeEventListener("icegatheringstatechange", mudou); limpezas.delete(limpar); erro ? nao(erro) : ok(); };
      const mudou = () => { if (pc.iceGatheringState === "complete") limpar(); };
      limpezas.add(limpar); pc.addEventListener("icegatheringstatechange", mudou); mudou();
    }));
  }
  function impressao(sdp) {
    const fps = [...sdp.matchAll(/^a=fingerprint:sha-256 ([0-9A-F:]+)\r?$/gmi)].map(m => m[1].replace(/:/g, "").toLowerCase());
    if (!fps.length || fps.some(fp => !/^[a-f0-9]{64}$/.test(fp) || fp !== fps[0])) throw new Error("chamada: impressão DTLS inválida");
    return fps[0].match(/../g).map(byte => parseInt(byte, 16));
  }
  function conferirDescricao(sdp) {
    if (typeof sdp !== "string" || !sdp.startsWith("v=0\r\n") || new TextEncoder().encode(sdp).length > 16 * 1024) throw new Error("chamada: SDP inválido");
    const midias = sdp.match(/^m=.*$/gm) ?? [];
    if (midias.length !== 1 || !midias[0].startsWith("m=application ")) throw new Error("chamada: somente DataChannels");
    for (const [nome, minimo] of [["ufrag", 4], ["pwd", 22]]) {
      const valores = [...sdp.matchAll(new RegExp(`^a=ice-${nome}:([^\\r\\n]*)\\r?$`, "gm"))].map(m => m[1]);
      if (!valores.length || valores.some(v => v.length < minimo || v.length > 256 || !/^[a-zA-Z0-9+/]+$/.test(v))) throw new Error("chamada: credencial ICE inválida");
    }
  }
  async function enviarDescricao(tipo) {
    await colherCandidatos();
    const sdp = pc.localDescription.sdp;
    conferirDescricao(sdp);
    await esperar(enviarSinal({ [tipo]: { sdp, impressao: impressao(sdp) } }));
  }
  async function iniciar() {
    conferirViva();
    if (iniciado) return;
    iniciado = true; mudar("conectando");
    try {
      conferirViva();
      if (papel === "quem-liga") {
        const oferta = await esperar(pc.createOffer());
        await esperar(pc.setLocalDescription(oferta));
        await enviarDescricao("Oferta");
      }
    } catch (erro) { fechar(erro); throw erro; }
  }
  async function receber(mensagem) {
    conferirViva();
    if (mensagem?.Oferta || mensagem?.Resposta) {
      const oferta = Boolean(mensagem.Oferta), descricao = mensagem.Oferta ?? mensagem.Resposta;
      if ((oferta ? papel !== "quem-recebe" || pc.signalingState !== "stable" || pc.remoteDescription : papel !== "quem-liga" || pc.signalingState !== "have-local-offer") ||
          typeof descricao.sdp !== "string" || new TextEncoder().encode(descricao.sdp).length > 16 * 1024 || !Array.isArray(descricao.impressao)) throw new Error("chamada: descrição inesperada");
      conferirDescricao(descricao.sdp);
      const fp = impressao(descricao.sdp);
      if (descricao.impressao.length !== 32 || fp.some((byte, i) => byte !== descricao.impressao[i])) throw new Error("chamada: impressão não corresponde ao SDP autenticado");
      await esperar(pc.setRemoteDescription({ type: oferta ? "offer" : "answer", sdp: descricao.sdp }));
      if (oferta) {
        await iniciar();
        await esperar(pc.setLocalDescription(await esperar(pc.createAnswer())));
        await enviarDescricao("Resposta");
      }
    } else if (mensagem?.Candidato) {
      const c = mensagem.Candidato;
      if (!pc.remoteDescription || typeof c.candidato !== "string" || new TextEncoder().encode(c.candidato).length > 2048 || (c.mid !== null && (typeof c.mid !== "string" || new TextEncoder().encode(c.mid).length > 64))) throw new Error("chamada: candidato inesperado");
      await esperar(pc.addIceCandidate({ candidate: c.candidato, sdpMid: c.mid, sdpMLineIndex: c.linha }));
    } else if (mensagem === "FimDosCandidatos") {
      if (!pc.remoteDescription) throw new Error("chamada: fim inesperado");
      await esperar(pc.addIceCandidate(null));
    } else throw new Error("chamada: sinal inválido");
  }
  function receberSinalAutenticado(mensagem) {
    const pedido = sinaisEmOrdem.then(() => receber(mensagem));
    sinaisEmOrdem = pedido.catch(erro => { fechar(erro); });
    return pedido;
  }
  sinal?.addEventListener("abort", abortar, { once: true });
  if (sinal?.aborted) abortar();
  return { pronto, iniciar, receberSinalAutenticado, controle, transporteDaVoz, tela, perfil, abortar,
    get estado() { return estado; }, get contas() { return { ...contas }; } };
}
