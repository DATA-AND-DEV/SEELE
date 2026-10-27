"use strict";

// Serviço autenticado. O encontro só recebe envelopes de sinalização; texto,
// voz e tela passam exclusivamente pelo transporte direto depois do aceite.
// A chave privada dura esta aba e nunca é exportada. Recarregar pede novo link.
async function criarServicoDeChamadasWeb({
  aoEstado = () => {}, aoTexto = () => {}, aoPar = () => {}, aoControle = () => {},
  urlDoEncontro = "https://rendzvous.seele.sh/chamada/1",
  iceServers = [{ urls: "stun:rendzvous.seele.sh:8384" }],
  wasm: wasmFornecido, buscar = (...args) => fetch(...args),
  criarEnlace = criarEnlaceDaChamadaWeb,
} = {}) {
  const wasm = wasmFornecido || await instanciarOWasmDaSessao();
  const encoder = new TextEncoder(), decoder = new TextDecoder();
  function operar(intencao) {
    const bytes = encoder.encode(JSON.stringify({ agora: Math.floor(Date.now() / 1000), ...intencao }));
    const ptr = wasm.alocar(bytes.length);
    try {
      new Uint8Array(wasm.memory.buffer).set(bytes, ptr);
      wasm.chamada_operar(ptr, bytes.length);
      const valor = JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer, wasm.saida_ptr(), wasm.saida_len()).slice()));
      if (valor.erro) throw new Error(`chamada: ${valor.erro}`);
      return valor;
    } finally { wasm.liberar(ptr, bytes.length); }
  }
  const chave = await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]);
  const publica = Array.from(new Uint8Array(await crypto.subtle.exportKey("raw", chave.publicKey)));
  const zero = () => Array(32).fill(0);
  const aleatorio = n => Array.from(crypto.getRandomValues(new Uint8Array(n)));
  const igual = (a, b) => Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((v, i) => v === b[i]);
  const idTexto = a => a.map(v => v.toString(16).padStart(2, "0")).join("");
  const vazio = () => ({ primeira: zero(), segunda: zero() });
  let epoca, conviteAtual = null, recepcao = null, chamada = null, nome = "", proximoTexto = 0;
  let criando = false, destruido = false;
  let retrato = Object.freeze({ estado: "livre" });
  const vistos = new Map();
  const chamadasVistas = new Set();
  const notificar = (f, ...args) => { try { f(...args); } catch { /* observador não governa recursos */ } };
  function mostrar(estado, motivo = null, contexto = chamada) {
    if (contexto && contexto !== chamada) return;
    retrato = Object.freeze({ estado, motivo, papel: contexto?.papel, apelido: contexto?.apelido });
    notificar(aoEstado, retrato);
  }
  function conferir(contexto) {
    if (destruido || contexto.abortador.signal.aborted) throw new DOMException("Chamada encerrada", "AbortError");
  }
  async function assinar(bytes) {
    const prova = new Uint8Array(await crypto.subtle.sign("Ed25519", chave.privateKey, new Uint8Array(bytes)));
    return { primeira: Array.from(prova.slice(0, 32)), segunda: Array.from(prova.slice(32)) };
  }
  async function http(texto, sinal) {
    const abortador = new AbortController();
    const abortar = () => abortador.abort();
    sinal?.addEventListener("abort", abortar, { once: true });
    if (sinal?.aborted) abortar();
    const prazo = setTimeout(abortar, 8000);
    try {
      const resposta = await buscar(urlDoEncontro, { method: texto === null ? "GET" : "POST",
        headers: texto === null ? {} : { "Content-Type": "text/plain;charset=UTF-8" },
        body: texto === null ? undefined : texto, cache: "no-store", credentials: "omit", signal: abortador.signal });
      if (!resposta.ok) { const erro = new Error(`chamada: HTTP ${resposta.status}`); erro.status = resposta.status; throw erro; }
      // Limite também antes de desserializar (Content-Length pode não existir).
      const leitor = resposta.body.getReader(); let total = 0; const partes = [];
      for (;;) { const { done, value } = await leitor.read(); if (done) break;
        total += value.length; if (total > 256 * 1024) { await leitor.cancel(); throw new Error("chamada: resposta grande"); }
        partes.push(value); }
      const bytes = new Uint8Array(total); let cursor = 0;
      for (const parte of partes) { bytes.set(parte, cursor); cursor += parte.length; }
      return operar({ acao: "resposta_ler", texto: decoder.decode(bytes) }).resposta;
    } finally { clearTimeout(prazo); sinal?.removeEventListener("abort", abortar); }
  }
  async function descobrir(sinal) {
    if (epoca) return;
    const resposta = await http(null, sinal);
    if (!resposta.Desafio?.epoca) throw new Error("chamada: encontro inválido");
    // A época limita replay no encontro. Não autentica a identidade de ninguém.
    epoca = resposta.Desafio.epoca;
  }
  async function pedido(contexto, operacao, opcoes = {}) {
    const terminal = opcoes.terminal ? new AbortController() : null;
    const prazo = terminal ? setTimeout(() => terminal.abort(), 3000) : null;
    try { return await pedidoAssinado(contexto, operacao, { ...opcoes, sinalTerminal: terminal?.signal }); }
    finally { clearTimeout(prazo); }
  }
  async function pedidoAssinado(contexto, operacao, { terminal = false, caixa = false, sinalTerminal } = {}) {
    const sinal = terminal ? sinalTerminal : contexto.abortador.signal;
    if (!terminal) conferir(contexto);
    await descobrir(sinal);
    if (!terminal) conferir(contexto);
    const p = { papel: contexto.papel === "quem-recebe" ? "QuemRecebe" : "QuemLiga",
      epoca, nonce: aleatorio(16), convite: contexto.convite.id,
      chamada: caixa ? zero() : contexto.id, remetente: publica,
      destinatario: caixa ? publica : contexto.par,
      geracao: contexto.geracao, sequencia: ++contexto.sequencia,
      valido_ate: Math.floor(Date.now() / 1000) + 60, operacao, assinatura: vazio() };
    p.assinatura = await assinar(operar({ acao: "pedido_mensagem", pedido: p }).bytes);
    if (!terminal) conferir(contexto);
    const texto = operar({ acao: "pedido_escrever", pedido: p }).texto;
    let resposta;
    // Uma repetição leva OS MESMOS bytes/nonce: o encontro retorna o resultado
    // guardado, inclusive quando o primeiro POST fez efeito e perdeu a resposta.
    for (let tentativa = 0; tentativa < 2; tentativa++) {
      try { resposta = await http(texto, sinal); break; }
      catch (erro) {
        if (tentativa || sinal?.aborted) throw erro;
        // O ponto aplica limite por IP. Não transforma Retry-After em loop.
        if (erro.status === 429) await new Promise((ok, nao) => {
          const abortar = () => { clearTimeout(timer); nao(new DOMException("Encerrada", "AbortError")); };
          const timer = setTimeout(() => { sinal?.removeEventListener("abort", abortar); ok(); }, 1000);
          sinal?.addEventListener("abort", abortar, { once: true });
          if (sinal?.aborted) abortar();
        });
      }
    }
    if (!terminal) conferir(contexto);
    if (resposta?.Recusa) throw new Error(`chamada: ${resposta.Recusa.motivo}`);
    return resposta;
  }
  function contexto(convite, papel, id = zero(), par = publica, geracao = 0) {
    return { convite, papel, id, par, geracao, sequencia: 0, remota: 0, cursor: 0,
      abortador: new AbortController(), fila: Promise.resolve(), enlace: null, admitida: false,
      timer: null, poll: null, sinais: [], encerrada: false, apelido: "" };
  }
  function enfileirar(c, operacao, opcoes) {
    const retorno = c.fila.then(() => pedido(c, operacao, opcoes));
    c.fila = retorno.catch(() => {});
    return retorno;
  }
  function terminar(c, motivo, avisar = true) {
    if (!c || c.encerrada) return false;
    c.encerrada = true;
    clearTimeout(c.timer); clearTimeout(c.poll);
    c.abortador.abort();
    try { c.enlace?.abortar(); } catch { /* limpa os demais recursos mesmo se o adaptador falhar */ }
    c.enlace = null; c.sinais.length = 0;
    if (chamada === c) { notificar(aoPar, null, { id: idTexto(c.id), papel: c.papel }); mostrar("encerrada", motivo, c); }
    if (avisar) {
      const pendente = c.papel === "quem-recebe" ? "Recusar" : "Cancelar";
      const operacao = c.admitida || c.aceitando ? "Encerrar" : pendente;
      // Independente do abortador da chamada; HTTP tem prazo próprio. Nunca
      // altera estado após terminar nem reutiliza o contexto da próxima chamada.
      void enfileirar(c, operacao, { terminal: true }).catch(erro => {
        // O aceite pode ter sido processado enquanto sua resposta HTTP se
        // perdeu/cancelou. Tenta o outro estado sem reabrir nem trocar geração.
        if (String(erro.message).endsWith("SemPermissao")) {
          return enfileirar(c, operacao === "Encerrar" ? pendente : "Encerrar", { terminal: true });
        }
      }).catch(() => {});
    }
    return true;
  }
  async function conectar(c) {
    conferir(c);
    if (c.enlace) return;
    c.admitida = true; clearTimeout(c.timer); mostrar("conectando", null, c);
    const enlace = criarEnlace({ papel: c.papel, sinal: c.abortador.signal, iceServers,
      enviarSinal: sinal => enfileirar(c, { Sinal: sinal }),
      // Uma queda local também precisa liberar a admissão no encontro. Sem
      // Encerrar, a próxima chamada pelo mesmo link ficava Ocupado até o TTL.
      aoEstado: estado => { if (estado === "encerrada" && !c.encerrada) terminar(c, "conexao"); } });
    c.enlace = enlace;
    enlace.controle.ouvir(bytes => {
      if (c.encerrada || c !== chamada) return;
      try {
        const controle = operar({ acao: "controle_ler", bytes: Array.from(bytes) }).controle;
        if (controle.Texto) notificar(aoTexto, { id: controle.Texto.id, de: "par", texto: controle.Texto.texto });
        else notificar(aoControle, controle);
      } catch { terminar(c, "controle-invalido"); }
    });
    try {
      await enlace.iniciar(); conferir(c);
      for (const sinal of c.sinais.splice(0)) await enlace.receberSinalAutenticado(sinal);
      await enlace.pronto; conferir(c);
      if (c !== chamada) { enlace.abortar(); return; }
      mostrar("dentro", null, c); notificar(aoPar, enlace, { id: idTexto(c.id), papel: c.papel, apelido: c.apelido });
    } catch (erro) { try { enlace.abortar(); } catch {} if (!c.encerrada) terminar(c, "conexao"); }
  }
  async function evento(caixa, p) {
    conferir(caixa);
    operar({ acao: "pedido_verificar", pedido: p });
    if (!igual(p.epoca, epoca) || !igual(p.convite, caixa.convite.id) || !igual(p.destinatario, publica)) throw new Error("chamada: contexto inválido");
    const nonce = idTexto(p.remetente) + idTexto(p.nonce);
    const agora = Math.floor(Date.now() / 1000);
    for (const [visto, ate] of vistos) if (ate <= agora) vistos.delete(visto);
    if (vistos.has(nonce)) return;
    if (vistos.size >= 1024) throw new Error("chamada: provas demais");
    vistos.set(nonce, p.valido_ate);
    if (p.operacao?.Chamar) {
      if (caixa !== recepcao || p.papel !== "QuemLiga" || !igual(p.operacao.Chamar.convite.dono, publica)
        || p.operacao.Chamar.convite.expira_em !== caixa.convite.expira_em
        || !igual(p.operacao.Chamar.convite.assinatura.primeira, caixa.convite.assinatura.primeira)
        || !igual(p.operacao.Chamar.convite.assinatura.segunda, caixa.convite.assinatura.segunda)) throw new Error("chamada: convite inválido");
      const id = idTexto(p.chamada);
      if (chamadasVistas.has(id)) return;
      if (chamadasVistas.size >= 256) throw new Error("chamada: convites demais");
      chamadasVistas.add(id);
      if (chamada && !chamada.encerrada) return; // 1:1; não substitui a conversa atual.
      const c = contexto(caixa.convite, "quem-recebe", p.chamada, p.remetente, p.geracao);
      c.remota = p.sequencia; c.apelido = p.operacao.Chamar.apelido; chamada = c;
      mostrar("tocando", null, c);
      c.timer = setTimeout(() => terminar(c, "nao-atendida"), Math.max(0, Math.min(45000, c.convite.expira_em * 1000 - Date.now())));
      return;
    }
    const c = chamada;
    if (!c || c.encerrada || !igual(p.chamada, c.id)) return;
    if (!igual(p.remetente, c.par) || p.papel !== (c.papel === "quem-liga" ? "QuemRecebe" : "QuemLiga")
      || p.geracao !== c.geracao || p.sequencia <= c.remota) throw new Error("chamada: prova fora de contexto");
    c.remota = p.sequencia;
    if (p.operacao === "Atender" && c.papel === "quem-liga" && !c.admitida) { void conectar(c); }
    else if (["Recusar", "Cancelar", "Encerrar"].includes(p.operacao)) terminar(c, p.operacao.toLowerCase(), false);
    else if (p.operacao?.Sinal) {
      if (!c.admitida && !c.aceitando) throw new Error("chamada: sinal antes do aceite");
      if (c.enlace) await c.enlace.receberSinalAutenticado(p.operacao.Sinal);
      else if (c.sinais.length < 8) c.sinais.push(p.operacao.Sinal);
      else throw new Error("chamada: sinais demais");
    } else throw new Error("chamada: transição inválida");
  }
  async function poll(c, caixa) {
    if (c.abortador.signal.aborted) return;
    try {
      const resposta = await enfileirar(c, { Buscar: { cursor: c.cursor, confirmado: c.cursor } }, { caixa });
      if (!resposta.Eventos) throw new Error("chamada: caixa inválida");
      for (const e of resposta.Eventos.eventos) { await evento(c, e.pedido); c.cursor = e.cursor; }
      c.poll = setTimeout(() => void poll(c, caixa), 2000);
    } catch (erro) {
      if (c.abortador.signal.aborted) return;
      // A conversa direta já estabelecida não depende da disponibilidade da
      // VPS. O DataChannel detecta a saída do par mesmo sem caixa de sinais.
      if (chamada && !chamada.encerrada && chamada.admitida && retrato.estado === "dentro") return;
      if (c === recepcao) { c.abortador.abort(); if (chamada && !chamada.encerrada && !chamada.admitida) terminar(chamada, "sinalizacao", false); else if (!chamada || chamada.encerrada) mostrar("encerrada", "sinalizacao", null); }
      else terminar(c, "sinalizacao", false);
    }
  }
  async function criarConvite(opcoes) {
    if (criando || destruido) throw new Error("chamada: indisponível");
    criando = true;
    try { return await prepararConvite(opcoes); } finally { criando = false; }
  }
  async function prepararConvite({ apelido, validadeSegundos = 3600 }) {
    if (chamada && !chamada.encerrada) throw new Error("chamada: ocupada");
    if (!Number.isInteger(validadeSegundos) || validadeSegundos < 1 || validadeSegundos > 86400) throw new Error("chamada: validade inválida");
    nome = String(apelido || "").trim();
    if (!nome || encoder.encode(nome).length > 128) throw new Error("chamada: nome inválido");
    if (recepcao) { recepcao.abortador.abort(); clearTimeout(recepcao.poll); }
    const convite = { id: aleatorio(32), dono: publica, expira_em: Math.floor(Date.now() / 1000) + validadeSegundos, assinatura: vazio() };
    convite.assinatura = await assinar(operar({ acao: "convite_mensagem", convite }).bytes);
    if (destruido) throw new DOMException("Chamada encerrada", "AbortError");
    const c = contexto(convite, "quem-recebe");
    recepcao = c;
    await enfileirar(c, { Registrar: { convite } }, { caixa: true });
    conferir(c); conviteAtual = convite; chamada = null;
    const fragmento = operar({ acao: "convite_escrever", convite }).fragmento;
    mostrar("recebendo", null, null); void poll(c, true);
    return { link: `${location.origin}/#chamada/${fragmento}`, expiraEm: convite.expira_em * 1000 };
  }
  function abrirConvite(link) {
    if (destruido || criando) throw new Error("chamada: indisponível");
    if (chamada && !chamada.encerrada) throw new Error("chamada: ocupada");
    const hash = new URL(link, location.href).hash;
    if (!hash.startsWith("#chamada/")) throw new Error("chamada: link inválido");
    const convite = operar({ acao: "convite_ler", fragmento: hash.slice(9) }).convite;
    if (igual(convite.dono, publica)) throw new Error("chamada: seu próprio convite");
    if (recepcao) { recepcao.abortador.abort(); clearTimeout(recepcao.poll); recepcao = null; }
    conviteAtual = convite;
    return { expiraEm: convite.expira_em * 1000, dono: idTexto(convite.dono) };
  }
  async function chamar({ apelido }) {
    if (destruido || criando || !conviteAtual || (chamada && !chamada.encerrada)) throw new Error("chamada: indisponível");
    operar({ acao: "convite_verificar", convite: conviteAtual });
    const c = contexto(conviteAtual, "quem-liga", aleatorio(32), conviteAtual.dono, crypto.getRandomValues(new Uint32Array(1))[0]);
    c.apelido = "Conversa pessoal"; chamada = c; mostrar("chamando", null, c);
    c.timer = setTimeout(() => terminar(c, "nao-atendida"), Math.max(0, Math.min(45000, c.convite.expira_em * 1000 - Date.now())));
    try { await enfileirar(c, { Chamar: { convite: c.convite, apelido: String(apelido || "").trim() } }); conferir(c); void poll(c, false); }
    catch (erro) { terminar(c, "sinalizacao"); throw erro; }
  }
  async function atender() {
    const c = chamada;
    if (!c || c.encerrada || retrato.estado !== "tocando") return false;
    operar({ acao: "convite_verificar", convite: c.convite });
    // Marca intenção antes do await: dois gestos não geram dois aceites.
    c.aceitando = true; mostrar("conectando", null, c);
    try { await enfileirar(c, "Atender"); conferir(c); void conectar(c); return true; }
    catch (erro) { terminar(c, "sinalizacao"); throw erro; }
  }
  async function enviarControle(controle) {
    const c = chamada;
    if (!c || c.encerrada || retrato.estado !== "dentro") throw new Error("chamada: desconectada");
    const bytes = operar({ acao: "controle_escrever", controle }).bytes;
    return c.enlace.controle.enviar(new Uint8Array(bytes));
  }
  return Object.freeze({ wasm, operar, criarConvite, abrirConvite, chamar, atender,
    ler: () => retrato,
    recusar: () => retrato.estado === "tocando" && terminar(chamada, "recusada"),
    cancelar: () => terminar(chamada, "cancelada"), encerrar: () => terminar(chamada, "encerrada"),
    async revogarConvite() {
      const c = recepcao; if (!c) return false;
      await enfileirar(c, "Revogar", { caixa: true });
      if (chamada && !chamada.encerrada && !chamada.admitida) terminar(chamada, "revogada", false);
      // Aceitas continuam: o polling da caixa ainda entrega sinais/encerramento.
      if (!chamada || chamada.encerrada) { c.abortador.abort(); clearTimeout(c.poll); mostrar("encerrada", "revogada", null); }
      return true;
    },
    enviarControle,
    async enviarTexto(texto) {
      const id = ++proximoTexto; await enviarControle({ Texto: { id, texto } }); notificar(aoTexto, { id, de: "eu", texto });
    },
    destruir() {
      destruido = true;
      terminar(chamada, "encerrada");
      if (recepcao) { recepcao.abortador.abort(); clearTimeout(recepcao.poll); }
    },
  });
}
