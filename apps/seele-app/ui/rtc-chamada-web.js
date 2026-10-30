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
  const MAX_CANDIDATOS = 64;
  const CAPACIDADE_REINICIO = "a=x-seele-ice-restart:1";
  let estado = "novo", iniciado = false, terminou = false, abriu = false;
  let sinaisEmOrdem = Promise.resolve();
  let saidaEmOrdem = Promise.resolve(), saidasPendentes = 0;
  let prazo, prazoTotal, prazoDeQueda, prazoDeRecuperacao;
  let etapa = "aguardando", motivoFinal = null, recuperando = false, reinicios = 0;
  let recuperacao = null, liberarRecuperacao;
  let aceitaReinicio = false, impressaoRemota = null, geracaoLocal = null, geracaoRemota = null;
  let descricaoEnfileirada = false;
  const candidatosLocais = [], candidatosRemotos = [], geracoesRemotas = new Set();
  const tiposLocais = {}, tiposRemotos = {};
  const limpezas = new Set();
  const contas = { vozDescartada: 0, esperasPorPressao: 0 };
  let resolverPronto, recusarPronto, recusarEncerrado;
  const pronto = new Promise((ok, nao) => { resolverPronto = ok; recusarPronto = nao; });
  const encerrado = new Promise((_, nao) => { recusarEncerrado = nao; });
  // São promessas públicas/competidoras; fechar antes de o consumidor esperar
  // não deixa rejeição sem observador. Quem as espera ainda recebe a falha.
  pronto.catch(() => {}); encerrado.catch(() => {});
  function marcarEtapa(proxima, ms = 30000) {
    etapa = proxima; clearTimeout(prazo);
    prazo = setTimeout(() => falhar(`prazo-${proxima}`), ms);
  }
  function falhar(motivo) {
    motivoFinal = motivo;
    fechar(new Error(`chamada: ${motivo}`));
  }
  function mudar(proximo) { estado = proximo; try { aoEstado(proximo); } catch { /* observador não é dono do transporte */ } }
  function fechar(motivo = new Error("chamada: encerrada")) {
    if (terminou) return;
    terminou = true;
    clearTimeout(prazo);
    clearTimeout(prazoTotal); clearTimeout(prazoDeQueda); clearTimeout(prazoDeRecuperacao);
    liberarRecuperacao?.(); recuperacao = null;
    candidatosLocais.length = 0; candidatosRemotos.length = 0;
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
    if (terminou || !Object.values(canais).every(c => c.readyState === "open")) return;
    if (recuperando && !["connected", "completed"].includes(pc.iceConnectionState)) return;
    abriu = true; recuperando = false; etapa = "aberta";
    liberarRecuperacao?.(); recuperacao = null;
    clearTimeout(prazo); clearTimeout(prazoTotal); clearTimeout(prazoDeQueda); clearTimeout(prazoDeRecuperacao);
    prazoDeQueda = null; prazoDeRecuperacao = null;
    if (estado !== "aberta") mudar("aberta"); resolverPronto();
  }
  function olharConexao() {
    if (terminou) return;
    if (pc.connectionState === "closed" || pc.iceConnectionState === "closed") { falhar("transporte-fechado"); return; }
    if (["connected", "completed"].includes(pc.iceConnectionState) && pc.connectionState === "connected") {
      conferirAbertura(); return;
    }
    const falhou = pc.iceConnectionState === "failed" || pc.connectionState === "failed";
    const caiu = pc.iceConnectionState === "disconnected" || pc.connectionState === "disconnected";
    if (!falhou && !caiu) return;
    // Antes de abrir, os prazos da sinalização/ICE continuam valendo. Uma
    // oscilação não transforma uma tentativa ainda viável em encerramento.
    if (!abriu) { if (falhou) falhar("ice-indisponivel"); return; }
    if (!recuperando) {
      recuperando = true; etapa = "recuperando"; mudar("recuperando");
      recuperacao = new Promise(ok => { liberarRecuperacao = ok; });
      prazoDeRecuperacao = setTimeout(() => falhar("prazo-recuperacao"), aceitaReinicio ? 45000 : 12000);
    }
    if (!prazoDeQueda && aceitaReinicio && papel === "quem-liga" && reinicios < 2) {
      prazoDeQueda = setTimeout(() => { void reiniciar().catch(() => falhar("reinicio-falhou")); }, falhou ? 0 : 5000);
    }
  }
  pc.addEventListener("connectionstatechange", olharConexao);
  pc.addEventListener("iceconnectionstatechange", olharConexao);
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
      if (voz && recuperando) { contas.vozDescartada++; return; }
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
        const conferirProgresso = () => {
          // O orçamento da recuperação é finito e governa também a fila SCTP.
          // Não derrubar a chamada por pressão enquanto ICE busca outro caminho.
          if (recuperando) { timer = setTimeout(conferirProgresso, 5000); return; }
          const erro = new Error("chamada: envio sem progresso"); terminar(erro); fechar(erro);
        };
        timer = setTimeout(conferirProgresso, 5000);
        andar();
      });
    }
    return {
      // Controle/tela devolvem Promise<boolean>; voz devolve boolean SÍNCRONO,
      // como criarVozDoNavegador espera, e nunca espera uma fila de SCTP.
      enviar(bytes) {
        const valido = bytes instanceof Uint8Array && bytes.byteLength <= teto;
        if (voz) {
          if (!valido || terminou || recuperando || canal.readyState !== "open" || canal.bufferedAmount >= LIMIAR) { contas.vozDescartada++; return false; }
          try { canal.send(bytes); return true; }
          catch (erro) { fechar(erro); return false; }
        }
        if (!valido) return Promise.reject(new TypeError("chamada: tamanho ou tipo inválido"));
        if (terminou) return Promise.reject(new Error("chamada: encerrada"));
        if (pedidosPendentes >= 256 || bytesPendentes + bytes.byteLength > MAX_FILA) return Promise.reject(new Error("chamada: fila de envio cheia"));
        pedidosPendentes++;
        const copia = bytes.slice(); bytesPendentes += copia.byteLength;
        const pedido = fila.then(async () => {
          await esperar(pronto);
          if (recuperacao) await esperar(recuperacao);
          await esperar(pressao());
          if (recuperacao) await esperar(recuperacao);
          canal.send(copia); return true;
        });
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

  function ufrag(sdp) {
    const valores = [...sdp.matchAll(/^a=ice-ufrag:([^\r\n]+)\r?$/gm)].map(m => m[1]);
    if (!valores.length || valores.some(v => v !== valores[0])) throw new Error("chamada: geração ICE inválida");
    return valores[0];
  }
  function contarCandidato(texto, tipos) {
    const tipo = texto.match(/ typ (host|srflx|prflx|relay)(?: |$)/)?.[1] || "outro";
    tipos[tipo] = (tipos[tipo] || 0) + 1;
  }
  function enfileirarSinal(mensagem) {
    if (terminou) return Promise.reject(new Error("chamada: encerrada"));
    if (saidasPendentes >= MAX_CANDIDATOS + 2) {
      falhar("sinais-demais"); return Promise.reject(new Error("chamada: sinais demais"));
    }
    saidasPendentes++;
    const pedido = saidaEmOrdem.then(() => { conferirViva(); return esperar(enviarSinal(mensagem)); });
    saidaEmOrdem = pedido.catch(() => { if (!terminou) falhar("sinalizacao"); });
    return pedido.finally(() => { saidasPendentes--; });
  }
  function escoarCandidatos() {
    if (!descricaoEnfileirada || terminou) return;
    for (const c of candidatosLocais.splice(0)) {
      if (c.geracao !== geracaoLocal) continue;
      // Não se espera a resposta HTTP aqui: a oferta já está na MESMA fila.
      // A fachada limita a pressão da caixa remota sem bloquear a coleta ICE.
      void enfileirarSinal({ Candidato: c.valor }).catch(() => {});
    }
  }
  pc.addEventListener("icecandidate", ({ candidate }) => {
    if (terminou || !candidate?.candidate) return;
    const texto = candidate.candidate;
    const geracao = candidate.usernameFragment || texto.match(/(?:^| )ufrag ([^ ]+)/)?.[1] || geracaoLocal;
    if (!geracao || (geracaoLocal && geracao !== geracaoLocal)) return;
    const candidato = /(?:^| )ufrag /.test(texto) ? texto : `${texto} ufrag ${geracao}`;
    if (new TextEncoder().encode(candidato).length > 2048 || candidatosLocais.length + saidasPendentes >= MAX_CANDIDATOS) {
      falhar("candidatos-demais"); return;
    }
    contarCandidato(candidato, tiposLocais);
    candidatosLocais.push({ geracao, valor: { candidato, mid: candidate.sdpMid, linha: candidate.sdpMLineIndex } });
    escoarCandidatos();
  });
  async function aplicarCandidato(c) {
    const geracao = c.candidato.match(/(?:^| )ufrag ([^ ]+)/)?.[1];
    // Pacotes atrasados da negociação anterior não encerram nem contaminam a nova.
    if (geracao && geracao !== geracaoRemota) {
      if (geracoesRemotas.has(geracao)) return;
      if (candidatosRemotos.length >= MAX_CANDIDATOS) throw new Error("chamada: candidatos demais");
      candidatosRemotos.push(c); return;
    }
    if (!pc.remoteDescription) {
      if (candidatosRemotos.length >= MAX_CANDIDATOS) throw new Error("chamada: candidatos demais");
      candidatosRemotos.push(c); return;
    }
    contarCandidato(c.candidato, tiposRemotos);
    await esperar(pc.addIceCandidate({ candidate: c.candidato, sdpMid: c.mid, sdpMLineIndex: c.linha,
      ...(geracao ? { usernameFragment: geracao } : {}) }));
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
    // A descrição sai imediatamente. STUN lento não segura endereços locais
    // ou IPv6 já disponíveis; os candidatos seguem por envelopes assinados.
    const sdp = pc.localDescription.sdp + (pc.localDescription.sdp.includes(CAPACIDADE_REINICIO) ? "" : `${CAPACIDADE_REINICIO}\r\n`);
    conferirDescricao(sdp);
    geracaoLocal = ufrag(sdp);
    const envio = enfileirarSinal({ [tipo]: { sdp, impressao: impressao(sdp) } });
    descricaoEnfileirada = true; escoarCandidatos();
    await envio;
  }
  async function ofertar(reinicio = false) {
    descricaoEnfileirada = false;
    const oferta = await esperar(pc.createOffer(reinicio ? { iceRestart: true } : undefined));
    geracaoLocal = ufrag(oferta.sdp);
    await esperar(pc.setLocalDescription(oferta));
    await enviarDescricao("Oferta");
  }
  async function reiniciar() {
    if (terminou || !recuperando || !aceitaReinicio || papel !== "quem-liga" || reinicios >= 2 || pc.signalingState !== "stable") return;
    reinicios++;
    await ofertar(true);
  }
  async function iniciar() {
    conferirViva();
    if (iniciado) return;
    iniciado = true; mudar("conectando"); marcarEtapa("sinalizacao");
    prazoTotal = setTimeout(() => falhar("prazo-total"), 60000);
    try { if (papel === "quem-liga") await ofertar(); }
    catch (erro) { fechar(erro); throw erro; }
  }
  async function receber(mensagem) {
    conferirViva();
    if (mensagem?.Oferta || mensagem?.Resposta) {
      const oferta = Boolean(mensagem.Oferta), descricao = mensagem.Oferta ?? mensagem.Resposta;
      const repetida = Boolean(pc.remoteDescription);
      if ((oferta ? papel !== "quem-recebe" || pc.signalingState !== "stable" : papel !== "quem-liga" || pc.signalingState !== "have-local-offer") ||
          typeof descricao.sdp !== "string" || new TextEncoder().encode(descricao.sdp).length > 16 * 1024 || !Array.isArray(descricao.impressao)) throw new Error("chamada: descrição inesperada");
      conferirDescricao(descricao.sdp);
      const fp = impressao(descricao.sdp), geracao = ufrag(descricao.sdp);
      if (descricao.impressao.length !== 32 || fp.some((byte, i) => byte !== descricao.impressao[i])) throw new Error("chamada: impressão não corresponde ao SDP autenticado");
      if (impressaoRemota && fp.some((byte, i) => byte !== impressaoRemota[i])) throw new Error("chamada: identidade DTLS mudou");
      if (repetida && (!aceitaReinicio || !abriu || geracoesRemotas.has(geracao) || (oferta && reinicios >= 2))) throw new Error("chamada: reinício inesperado");
      if (repetida && oferta) reinicios++;
      impressaoRemota = fp;
      aceitaReinicio = descricao.sdp.split(/\r?\n/).includes(CAPACIDADE_REINICIO);
      await iniciar();
      await esperar(pc.setRemoteDescription({ type: oferta ? "offer" : "answer", sdp: descricao.sdp }));
      geracaoRemota = geracao; geracoesRemotas.add(geracao);
      if (!abriu) marcarEtapa("ice");
      for (const c of candidatosRemotos.splice(0)) await aplicarCandidato(c);
      if (oferta) {
        descricaoEnfileirada = false;
        const resposta = await esperar(pc.createAnswer());
        geracaoLocal = ufrag(resposta.sdp);
        await esperar(pc.setLocalDescription(resposta));
        await enviarDescricao("Resposta");
      }
    } else if (mensagem?.Candidato) {
      const c = mensagem.Candidato;
      if (typeof c.candidato !== "string" || !c.candidato.startsWith("candidate:") || new TextEncoder().encode(c.candidato).length > 2048 ||
          (c.mid !== null && (typeof c.mid !== "string" || new TextEncoder().encode(c.mid).length > 64)) ||
          (c.linha !== null && (!Number.isInteger(c.linha) || c.linha < 0 || c.linha > 65535))) throw new Error("chamada: candidato inesperado");
      await aplicarCandidato(c);
    } else if (mensagem === "FimDosCandidatos") {
      // Compatibilidade com pares antigos; durante restart não aplicar marcador
      // sem geração à negociação seguinte. A conexão não depende desse marcador.
      if (pc.remoteDescription && !reinicios) await esperar(pc.addIceCandidate(null));
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
    get estado() { return estado; },
    get diagnostico() { return { etapa, motivo: motivoFinal, ice: pc.iceConnectionState, conexao: pc.connectionState, reinicios, candidatosLocais: { ...tiposLocais }, candidatosRemotos: { ...tiposRemotos } }; },
    get contas() { return { ...contas }; } };
}
