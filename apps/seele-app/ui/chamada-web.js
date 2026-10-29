"use strict";

// Perfil opcional da conexão: limite antes de decodificar a imagem.
function nomeDoPerfilPessoalValido(apelido) {
  return typeof apelido === "string" && Boolean(apelido.trim()) && new TextEncoder().encode(apelido).length <= 128 && !/[\u0000-\u001f\u007f-\u009f]/u.test(apelido);
}
async function validarPerfilDaChamadaPessoal(perfil) {
  if (!perfil || perfil.v !== 1 || Object.keys(perfil).sort().join(",") !== "apelido,retrato,v" || !nomeDoPerfilPessoalValido(perfil.apelido)) throw new Error("chamada: perfil inválido");
  if (perfil.retrato !== null) {
    if (typeof perfil.retrato !== "string" || perfil.retrato.length > 11000 || !/^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/.test(perfil.retrato)) throw new Error("chamada: retrato inválido");
    const bytes = Uint8Array.from(atob(perfil.retrato.slice(22)), c => c.charCodeAt(0));
    if (bytes.length > 8192 || bytes.length < 33 || [137,80,78,71,13,10,26,10].some((b,i) => bytes[i] !== b)) throw new Error("chamada: retrato inválido");
    const header = new DataView(bytes.buffer), largura = header.getUint32(16), altura = header.getUint32(20);
    if (header.getUint32(8) !== 13 || header.getUint32(12) !== 0x49484452 || !largura || !altura || largura > 128 || altura > 128) throw new Error("chamada: retrato inválido");
    const imagem = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
    try { if (imagem.width !== largura || imagem.height !== altura) throw new Error("chamada: retrato inválido"); }
    finally { imagem.close(); }
  }
  return perfil;
}


// Serviço autenticado. O encontro só recebe envelopes de sinalização; texto,
// voz e tela passam exclusivamente pelo transporte direto depois do aceite.
// A identidade pessoal é durável no IndexedDB; a chave nunca é exportável.
async function criarServicoDeChamadasWeb({
  obterPerfil = async apelido => ({ apelido, retrato: null }), lerPerfil = async () => null, aoPerfil = () => {},
  aoEstado = () => {}, aoRecepcao = () => {}, aoTexto = () => {}, aoPar = () => {}, aoControle = () => {}, aoDiagnostico = () => {},
  urlDoEncontro, iceServers,
  wasm: wasmFornecido, buscar = (...args) => fetch(...args),
  criarEnlace = criarEnlaceDaChamadaWeb,
  origemDosLinks,
  identidade: identidadeFornecida,
  conferirPar = typeof conferirParPessoalWeb === "function" ? conferirParPessoalWeb : null,
  reservarChamada = async () => {},
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
  // As três fontes canônicas pertencem ao protocolo Rust; a bancada pode
  // fornecer origens/transportes locais explicitamente, sem trocar o padrão.
  const configuracao = operar({ acao: "configuracao" });
  if (urlDoEncontro === undefined) urlDoEncontro = configuracao.url_do_encontro;
  if (iceServers === undefined) iceServers = configuracao.ice_servers;
  if (origemDosLinks === undefined) origemDosLinks = configuracao.origem_dos_links;
  const identidade = identidadeFornecida || (typeof identidadePessoalDoNavegador === "function"
    ? await identidadePessoalDoNavegador() : null);
  const chave = identidade?.chave || (identidade?.assinar ? null : await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]));
  const publica = identidade?.publica || Array.from(new Uint8Array(await crypto.subtle.exportKey("raw", chave.publicKey)));
  const codigo = operar({ acao: "codigo_da_chave", chave: publica }).codigo;
  const zero = () => Array(32).fill(0);
  const aleatorio = n => Array.from(crypto.getRandomValues(new Uint8Array(n)));
  const igual = (a, b) => Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((v, i) => v === b[i]);
  const idTexto = a => a.map(v => v.toString(16).padStart(2, "0")).join("");
  const vazio = () => ({ primeira: zero(), segunda: zero() });
  let epoca, conviteAtual = null, recepcao = null, chamada = null, nome = "", proximoTexto = 0;
  let criando = false, destruido = false, buscaEmCurso = null, preparandoCodigo = null, codigoAtivo = false;
  const recepcoes = new Set();
  let retratoDaRecepcao = null, geracaoDaRecepcao = 0, geracaoDoPreparo = null;
  const esperasDaRecepcao = [2000, 5000, 15000];
  let retrato = Object.freeze({ estado: "livre" });
  const vistos = new Map();
  const chamadasVistas = new Map();
  const notificar = (f, ...args) => { try { f(...args); } catch { /* observador não governa recursos */ } };
  function mostrar(estado, motivo = null, contexto = chamada) {
    if (contexto && contexto !== chamada) return;
    retrato = Object.freeze({ estado, motivo, id: contexto ? idTexto(contexto.id) : null, codigo: contexto?.codigo, papel: contexto?.papel, apelido: contexto?.apelido });
    notificar(aoEstado, retrato);
  }
  async function receberPerfil(c, bytes) {
    if (c !== chamada || c.encerrada || !c.admitida || c.perfilRecebido || bytes.length > 12000) return;
    c.perfilRecebido = true;
    try {
      const perfil = await validarPerfilDaChamadaPessoal(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)));
      conferir(c);
      if (c !== chamada) return;
      if (conferirPar) await conferirPar(c.codigo, c.par, perfil.apelido, c.papel === "quem-liga" ? "liguei" : "recebi");
      conferir(c);
      if (c !== chamada) return;
      c.apelido = perfil.apelido;
      mostrar(retrato.estado, retrato.motivo, c);
      await aoPerfil({ codigo: c.codigo, publica: c.par.slice(), ...perfil });
    } catch { /* Um perfil opcional não derruba uma conversa estabelecida. */ }
  }
  function conferir(contexto) {
    if (destruido || contexto.abortador.signal.aborted) throw new DOMException("Chamada encerrada", "AbortError");
  }
  async function assinar(bytes, intencao) {
    if (identidade?.assinar) return identidade.assinar(intencao);
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
  async function pedidoAssinado(contexto, operacao, { terminal = false, caixa = false, sinalTerminal, epocaRepetida = false } = {}) {
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
    p.assinatura = await assinar(operar({ acao: "pedido_mensagem", pedido: p }).bytes, { Pedido: p });
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
    if (resposta?.Recusa) {
      if (resposta.Recusa.motivo === "Epoca" && !epocaRepetida) {
        epoca = null;
        return pedidoAssinado(contexto, operacao, { terminal, caixa, sinalTerminal, epocaRepetida: true });
      }
      const erro = new Error(`chamada: ${resposta.Recusa.motivo}`);
      erro.motivo = resposta.Recusa.motivo;
      throw erro;
    }
    return resposta;
  }
  function estadoDaRecepcao(estado, motivo = null) {
    if (retratoDaRecepcao?.estado === estado && retratoDaRecepcao?.motivo === motivo) return;
    retratoDaRecepcao = Object.freeze({ estado, motivo }); notificar(aoRecepcao, retratoDaRecepcao);
  }
  function soltarRecepcao(c) {
    if (!c) return;
    clearTimeout(c.poll); clearTimeout(c.renovar); c.abortador.abort(); recepcoes.delete(c);
  }
  function renovarDepois(c) {
    clearTimeout(c.renovar);
    if (!codigoAtivo || c !== recepcao || c.revogando || destruido) return;
    c.renovar = setTimeout(() => {
      void receberPorCodigo({ apelido: nome, forcar: true }).catch(erro => falhaDaRecepcao(c, erro, true));
    }, Math.max(1000, (c.convite.expira_em - 60) * 1000 - Date.now()));
  }
  function falhaDaRecepcao(c, erro, renovacao = false) {
    if (c.abortador.signal.aborted || c.revogando || destruido) return;
    c.falha = true;
    if (c === recepcao) estadoDaRecepcao("indisponivel", erro.motivo || erro.status || "rede");
    const repetivel = erro.motivo === "SemRegistro" || erro.motivo === "Epoca"
      || erro.status === 429 || erro.status >= 500 || (!erro.status && !String(erro.message).startsWith("chamada:"));
    const continuada = (c.falhas || 0) >= esperasDaRecepcao.length;
    const admitida = chamada && !chamada.encerrada && chamada.admitida;
    const espera = esperasDaRecepcao[c.falhas || 0] ?? (admitida ? 15000 : undefined);
    c.falhas = Math.min((c.falhas || 0) + 1, esperasDaRecepcao.length);
    if (!repetivel || espera === undefined) { c.pausada = true; return; }
    c.poll = setTimeout(() => {
      // Só a chamada admitida sustenta tentativas além do orçamento ocioso.
      if (continuada && (!chamada || chamada.encerrada || !chamada.admitida)) { c.pausada = true; return; }
      if (renovacao || c.convite.expira_em <= Math.floor(Date.now() / 1000)) {
        void receberPorCodigo({ apelido: nome, forcar: true }).catch(e => falhaDaRecepcao(c, e, true));
      } else {
        void (async () => {
          try {
            await enfileirar(c, { Registrar: { convite: c.convite } }, { caixa: true });
            c.cursor = 0;
            await poll(c, true);
          } catch (e) { falhaDaRecepcao(c, e); }
        })();
      }
    }, espera);
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
  function esperarSinal(c, ms) {
    return new Promise((ok, nao) => {
      const abortar = () => { clearTimeout(timer); nao(new DOMException("Chamada encerrada", "AbortError")); };
      const timer = setTimeout(() => { c.abortador.signal.removeEventListener("abort", abortar); ok(); }, ms);
      c.abortador.signal.addEventListener("abort", abortar, { once: true });
      if (c.abortador.signal.aborted) abortar();
    });
  }
  async function enviarSinalDaChamada(c, sinal) {
    // A caixa publicada comporta oito eventos. Candidatos não podem monopolizar
    // a fila de pedidos: polls precisam continuar drenando a outra ponta.
    if (sinal.Candidato) await esperarSinal(c, 500);
    for (let tentativa = 0; ; tentativa++) {
      conferir(c);
      try { return await enfileirar(c, { Sinal: sinal }); }
      catch (erro) {
        if (erro.motivo !== "Cheio" || tentativa >= 5 || c.abortador.signal.aborted) throw erro;
        // Pedido novo só após recusa explícita: o mesmo nonce repetiria a
        // recusa em cache. Retry de resposta HTTP perdida segue idempotente.
        await esperarSinal(c, 1000);
      }
    }
  }
  function terminar(c, motivo, avisar = true) {
    if (!c || c.encerrada) return false;
    c.encerrada = true;
    void Promise.resolve(reservarChamada(idTexto(c.id), false)).catch(() => {});
    clearTimeout(c.timer); clearTimeout(c.poll);
    c.abortador.abort();
    try { c.enlace?.abortar(); } catch { /* limpa os demais recursos mesmo se o adaptador falhar */ }
    c.enlace = null; c.sinais.length = 0;
    for (const caixa of recepcoes) if (caixa !== recepcao) soltarRecepcao(caixa);
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
      enviarSinal: sinal => enviarSinalDaChamada(c, sinal),
      // Uma queda local também precisa liberar a admissão no encontro. Sem
      // Encerrar, a próxima chamada pelo mesmo link ficava Ocupado até o TTL.
      aoEstado: estado => {
        if (c.encerrada) return;
        if (estado === "encerrada") {
          notificar(aoDiagnostico, c.enlace?.diagnostico || { etapa: "sinalizacao", motivo: "transporte-encerrado" });
          terminar(c, "conexao");
        } else if (estado === "recuperando" || estado === "aberta") {
          notificar(aoDiagnostico, c.enlace?.diagnostico || { etapa: estado });
        }
      } });
    c.enlace = enlace;
    enlace.perfil?.ouvir(bytes => { void receberPerfil(c, bytes); });
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
      // O perfil só sai depois da ligação entre SDP assinado e par DTLS.
      if (enlace.perfil) void (async () => {
        try {
          const perfil = await validarPerfilDaChamadaPessoal({ ...(await obterPerfil(c.nomeLocal)), v: 1 });
          conferir(c);
          if (c === chamada) enlace.perfil.enviar(encoder.encode(JSON.stringify(perfil)));
        } catch { /* Falta de retrato ou nome local não derruba a conversa. */ }
      })();
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
      if (p.papel !== "QuemLiga" || !igual(p.operacao.Chamar.convite.dono, publica)
        || p.operacao.Chamar.convite.expira_em !== caixa.convite.expira_em
        || !igual(p.operacao.Chamar.convite.assinatura.primeira, caixa.convite.assinatura.primeira)
        || !igual(p.operacao.Chamar.convite.assinatura.segunda, caixa.convite.assinatura.segunda)) throw new Error("chamada: convite inválido");
      const id = idTexto(p.chamada);
      for (const [visto, ate] of chamadasVistas) if (ate <= agora) chamadasVistas.delete(visto);
      if (chamadasVistas.has(id)) return;
      if (chamadasVistas.size >= 256) throw new Error("chamada: convites demais");
      chamadasVistas.set(id, p.valido_ate + 120);
      if (caixa !== recepcao || caixa.revogando || (chamada && !chamada.encerrada)) {
        const ocupada = contexto(caixa.convite, "quem-recebe", p.chamada, p.remetente, p.geracao);
        void enfileirar(ocupada, "Recusar", { terminal: true }).catch(() => {});
        return;
      }
      const c = contexto(caixa.convite, "quem-recebe", p.chamada, p.remetente, p.geracao);
      c.remota = p.sequencia; c.apelido = p.operacao.Chamar.apelido; c.nomeLocal = nome;
      c.codigo = operar({ acao: "codigo_da_chave", chave: p.remetente }).codigo;
      if (conferirPar) {
        try { await conferirPar(c.codigo, p.remetente, c.apelido, "perdida"); await reservarChamada(idTexto(c.id)); }
        catch { void enfileirar(c, "Recusar", { terminal: true }).catch(() => {}); return; }
        if (caixa.abortador.signal.aborted || destruido) { await reservarChamada(idTexto(c.id), false); return; }
      }
      if (chamada && !chamada.encerrada) {
        await reservarChamada(idTexto(c.id), false);
        void enfileirar(c, "Recusar", { terminal: true }).catch(() => {});
        return;
      }
      chamada = c;
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
      c.falha = false; c.pausada = false; c.falhas = 0;
      if (c === recepcao && !c.revogando) estadoDaRecepcao("registrada");
      c.poll = setTimeout(() => void poll(c, caixa), 2000);
    } catch (erro) {
      if (c.abortador.signal.aborted) return;
      // Reconectar sinalização nunca recria RTC, captura ou uma chamada.
      if (recepcoes.has(c)) {
        falhaDaRecepcao(c, erro);
        if (chamada && !chamada.encerrada && !chamada.admitida) terminar(chamada, "sinalizacao", false);
      } else if (c.admitida && ["dentro", "conectando"].includes(retrato.estado)
          && (erro.status === 429 || erro.status >= 500 || (!erro.status && !String(erro.message).startsWith("chamada:")))) {
        const espera = esperasDaRecepcao[c.falhas || 0] ?? 15000;
        c.falhas = Math.min((c.falhas || 0) + 1, esperasDaRecepcao.length);
        c.poll = setTimeout(() => void poll(c, caixa), espera);
      } else terminar(c, "sinalizacao", false);
    }
  }
  async function criarConvite(opcoes) {
    if (preparandoCodigo) await preparandoCodigo.catch(() => {});
    if (criando || destruido) throw new Error("chamada: indisponível");
    criando = true;
    try { return await prepararConvite(opcoes); } finally { criando = false; }
  }
  async function prepararConvite({ apelido, validadeSegundos = 3600, preservarChamada = false }) {
    const geracaoDoRegistro = geracaoDaRecepcao;
    if (!preservarChamada && chamada && !chamada.encerrada) throw new Error("chamada: ocupada");
    if (!Number.isInteger(validadeSegundos) || validadeSegundos < 1 || validadeSegundos > 86400) throw new Error("chamada: validade inválida");
    nome = String(apelido || "").trim();
    if (!nome || encoder.encode(nome).length > 128) throw new Error("chamada: nome inválido");
    const velha = recepcao;
    const convite = { id: aleatorio(32), dono: publica, expira_em: Math.floor(Date.now() / 1000) + validadeSegundos, assinatura: vazio() };
    convite.assinatura = await assinar(operar({ acao: "convite_mensagem", convite }).bytes, { Convite: convite });
    if (destruido || geracaoDoRegistro !== geracaoDaRecepcao) throw new DOMException("Recepção cancelada", "AbortError");
    const c = contexto(convite, "quem-recebe");
    c.registrando = true;
    recepcoes.add(c);
    try { await enfileirar(c, { Registrar: { convite } }, { caixa: true }); conferir(c); }
    catch (erro) { soltarRecepcao(c); throw erro; }
    finally { c.registrando = false; }
    if (geracaoDoRegistro !== geracaoDaRecepcao || c.revogando) {
      // Revogar entrou na mesma fila, depois de Registrar. Mesmo se o servidor
      // concluiu tarde, a resposta não reinstala a caixa nem volta ao polling.
      try { await c.revogacao; } finally { soltarRecepcao(c); }
      throw new DOMException("Recepção cancelada", "AbortError");
    }
    recepcao = c;
    if (velha) {
      clearTimeout(velha.renovar);
      if (!chamada || chamada.encerrada || !igual(chamada.convite.id, velha.convite.id)) soltarRecepcao(velha);
    }
    if (!chamada || chamada.encerrada) { conviteAtual = convite; chamada = null; mostrar("recebendo", null, null); }
    estadoDaRecepcao("registrada"); renovarDepois(c);
    const fragmento = operar({ acao: "convite_escrever", convite }).fragmento;
    void poll(c, true);
    return { link: `${origemDosLinks}/#chamada/${fragmento}`, expiraEm: convite.expira_em * 1000 };
  }
  function abrirConvite(link) {
    if (destruido || criando) throw new Error("chamada: indisponível");
    if (chamada && !chamada.encerrada) throw new Error("chamada: ocupada");
    const texto = String(link).trim();
    const origem = new URL(origemDosLinks);
    // O fragmento entregue pela própria página também funciona na casca nativa.
    const endereco = texto.startsWith("#") ? new URL(texto, origem) : new URL(texto);
    const host = endereco.hostname.toLowerCase().replace(/\.$/, "");
    const aposentado = host === "seele.app.br" || host.endsWith(".seele.app.br");
    if (aposentado || endereco.origin !== origem.origin || origem.origin === "null"
        || endereco.username || endereco.password || !["https:", "http:"].includes(endereco.protocol)) {
      const erro = new Error(aposentado
        ? "Este endereço foi aposentado. Peça um novo convite."
        : "Este convite usa outro endereço. Peça um novo convite.");
      erro.origemDoConviteInvalida = true;
      throw erro;
    }
    const hash = endereco.hash;
    if (!hash.startsWith("#chamada/")) throw new Error("chamada: link inválido");
    const convite = operar({ acao: "convite_ler", fragmento: hash.slice(9) }).convite;
    if (igual(convite.dono, publica)) throw new Error("chamada: seu próprio convite");
    if (recepcao && !codigoAtivo) { soltarRecepcao(recepcao); recepcao = null; }
    conviteAtual = convite;
    return { expiraEm: convite.expira_em * 1000, dono: idTexto(convite.dono) };
  }
  async function chamar({ apelido }) {
    if (destruido || criando || !conviteAtual || (chamada && !chamada.encerrada)) throw new Error("chamada: indisponível");
    operar({ acao: "convite_verificar", convite: conviteAtual });
    const c = contexto(conviteAtual, "quem-liga", aleatorio(32), conviteAtual.dono, crypto.getRandomValues(new Uint32Array(1))[0]);
    c.codigo = operar({ acao: "codigo_da_chave", chave: c.par }).codigo;
    if (!nomeDoPerfilPessoalValido(apelido)) throw new Error("chamada: nome inválido");
    c.apelido = "Outra pessoa"; c.nomeLocal = apelido.trim(); chamada = c; mostrar("chamando", null, c);
    c.timer = setTimeout(() => terminar(c, "nao-atendida"), Math.max(0, Math.min(45000, c.convite.expira_em * 1000 - Date.now())));
    try {
      if (conferirPar) await conferirPar(c.codigo, c.par, null, "liguei");
      conferir(c);
      try {
        const anterior = await lerPerfil(c.codigo, c.par);
        conferir(c);
        if (nomeDoPerfilPessoalValido(anterior?.apelido)) { c.apelido = anterior.apelido; mostrar("chamando", null, c); }
      } catch (erro) { conferir(c); }
      conferir(c);
      await reservarChamada(idTexto(c.id));
      if (c.encerrada || destruido) { await reservarChamada(idTexto(c.id), false); conferir(c); }
      await enfileirar(c, { Chamar: { convite: c.convite, apelido: String(apelido || "").trim() } });
      conferir(c); void poll(c, false);
    } catch (erro) { terminar(c, "sinalizacao"); throw erro; }
  }

  async function atender({ apelido = nome } = {}) {
    const c = chamada;
    if (!c || c.encerrada || retrato.estado !== "tocando") return false;
    operar({ acao: "convite_verificar", convite: c.convite });
    if (!nomeDoPerfilPessoalValido(apelido)) throw new Error("chamada: nome inválido");
    c.nomeLocal = apelido.trim();
    // Marca intenção antes do await: dois gestos não geram dois aceites.
    c.aceitando = true; mostrar("conectando", null, c);
    try {
      await enfileirar(c, "Atender"); conferir(c);
      if (conferirPar) await conferirPar(c.codigo, c.par, c.apelido, "recebi");
      conferir(c); void conectar(c); return true;
    }
    catch (erro) { terminar(c, "sinalizacao"); throw erro; }
  }
  async function enviarControle(controle) {
    const c = chamada;
    if (!c || c.encerrada || retrato.estado !== "dentro") throw new Error("chamada: desconectada");
    const bytes = operar({ acao: "controle_escrever", controle }).bytes;
    return c.enlace.controle.enviar(new Uint8Array(bytes));
  }
  async function receberPorCodigo({ apelido, forcar = false }) {
    codigoAtivo = true;
    const geracaoDesejada = geracaoDaRecepcao;
    if (preparandoCodigo) {
      if (geracaoDoPreparo === geracaoDesejada) return preparandoCodigo;
      // OFF invalidou o preparo. ON espera sua limpeza, não reutiliza o
      // convite cancelado e só inicia se essa preferência ainda for atual.
      await preparandoCodigo.catch(() => {});
      if (destruido || !codigoAtivo || geracaoDesejada !== geracaoDaRecepcao) throw new DOMException("Recepção cancelada", "AbortError");
      return receberPorCodigo({ apelido, forcar });
    }
    if (!forcar && recepcao && !recepcao.falha && !recepcao.pausada && !recepcao.revogando && !recepcao.abortador.signal.aborted && recepcao.convite.expira_em > Math.floor(Date.now() / 1000) + 60) return { codigo };
    estadoDaRecepcao("preparando");
    const geracaoDoRegistro = geracaoDaRecepcao;
    const promessa = criarConvite({ apelido, validadeSegundos: 86400, preservarChamada: true }).then(() => ({ codigo }))
      .catch(erro => { if (geracaoDoRegistro === geracaoDaRecepcao) estadoDaRecepcao("indisponivel", erro.motivo || erro.status || "rede"); throw erro; })
      .finally(() => { if (preparandoCodigo === promessa) preparandoCodigo = null; });
    preparandoCodigo = promessa;
    geracaoDoPreparo = geracaoDoRegistro;
    return promessa;
  }
  async function chamarCodigo({ codigo: pedido, apelido }) {
    if (destruido || criando || buscaEmCurso || (chamada && !chamada.encerrada)) throw new Error("chamada: indisponível");
    const alvo = operar({ acao: "codigo_ler", codigo: String(pedido || "").replace(/^@/, "") }).codigo;
    if (alvo === codigo) throw new Error("chamada: seu próprio código");
    const c = contexto({ id: publica }, "quem-liga");
    buscaEmCurso = c; mostrar("procurando", null, null);
    try {
      const resposta = await enfileirar(c, { Localizar: { codigo: alvo } }, { caixa: true });
      conferir(c);
      const convite = resposta.Localizado?.convite;
      if (!convite) throw new Error("chamada: código indisponível");
      operar({ acao: "convite_verificar", convite });
      if (operar({ acao: "codigo_da_chave", chave: convite.dono }).codigo !== alvo) throw new Error("chamada: código não confere");
      if (conferirPar) await conferirPar(alvo, convite.dono);
      conferir(c);
      if (destruido || criando || (chamada && !chamada.encerrada)) throw new Error("chamada: indisponível");
      // Mantém a própria caixa ativa para continuar recebendo depois da saída.
      conviteAtual = convite;
      return await chamar({ apelido });
    } catch (erro) {
      if (buscaEmCurso === c && !c.abortador.signal.aborted && (!chamada || chamada.encerrada) && retrato.estado === "procurando") mostrar("encerrada", "sinalizacao", null);
      throw erro;
    } finally { if (buscaEmCurso === c) buscaEmCurso = null; c.abortador.abort(); }
  }
  function cancelarAtual(motivo) {
    let tinhaBusca = false;
    if (buscaEmCurso) { buscaEmCurso.abortador.abort(); buscaEmCurso = null; tinhaBusca = true; mostrar("encerrada", motivo, null); }
    return terminar(chamada, motivo) || tinhaBusca;
  }
  return Object.freeze({ wasm, operar, codigo, receberPorCodigo, chamarCodigo, criarConvite, abrirConvite, chamar, atender,
    ler: () => retrato,
    recusar: () => retrato.estado === "tocando" && terminar(chamada, "recusada"),
    cancelar: () => cancelarAtual("cancelada"), encerrar: () => cancelarAtual("encerrada"),
    async revogarConvite() {
      codigoAtivo = false;
      const geracaoDaRevogacao = ++geracaoDaRecepcao;
      const chamadaRevogada = chamada;
      estadoDaRecepcao("desligada");
      const caixas = [...recepcoes];
      if (!caixas.length) return false;
      const revogacoes = caixas.map(c => {
        c.revogando = true; clearTimeout(c.renovar);
        const aceita = chamada && !chamada.encerrada && chamada.admitida && igual(chamada.convite.id, c.convite.id);
        if (!aceita) {
          clearTimeout(c.poll);
          // Registrar já enviado precisa concluir antes de Revogar; abortar o
          // fetch aqui perderia a confirmação de um registro aplicado tarde.
          if (!c.registrando) c.abortador.abort();
        }
        c.revogacao ??= enfileirar(c, "Revogar", { caixa: true, terminal: true });
        return c.revogacao.finally(() => {
          if (!aceita) { soltarRecepcao(c); if (recepcao === c) recepcao = null; }
        });
      });
      try { await Promise.all(revogacoes); }
      finally {
        if (chamadaRevogada && !chamadaRevogada.encerrada && !chamadaRevogada.admitida) terminar(chamadaRevogada, "revogada", false);
        if (geracaoDaRevogacao === geracaoDaRecepcao && (!chamada || chamada.encerrada)) mostrar("encerrada", "revogada", null);
      }
      return true;
    },
    enviarControle,
    async enviarTexto(texto) {
      const atual = chamada, id = ++proximoTexto;
      await enviarControle({ Texto: { id, texto } });
      if (atual !== chamada || atual.encerrada) return;
      notificar(aoTexto, { id, de: "eu", texto });
    },
    destruir() {
      destruido = true;
      cancelarAtual("encerrada");
      codigoAtivo = false;
      for (const c of recepcoes) soltarRecepcao(c);
      estadoDaRecepcao("desligada");
    },
  });
}
