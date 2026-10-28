// Integra a conversa por link sem criar uma sessão fictícia de servidor.
"use strict";

function instalarConversasPessoaisWeb() {
  const navegador = PONTE_NO_LUGAR_DO_TAURI;
  const chamarNativo = (comando, args = {}) => window.__TAURI__.core.invoke(comando, args);
  const entrada = document.getElementById("boot-conversa-pessoal");
  if (!entrada) return;
  let servico = null, preparando = null, voz = null, preparandoVoz = null, tela = null, par = null;
  let geracao = 0, geracaoDaRecepcao = 0, geracaoDaMidia = 0, microfone = false, abrindoMicrofone = false;
  let apelidoDaConversa = "";
  let fase = "livre", linkDeEntrada = null, conviteNoServico = null, recebendoCodigo = true;
  let estadoDoCodigoDireto = { estado: "Subindo" }, criandoLink = false, preparoDoConvite = null;
  const viva = () => ["procurando", "tocando", "chamando", "conectando", "dentro"].includes(fase);
  const vista = criarVistaDaConversaPessoal({ acoes: {
    voltar() { terminar(); vista.fechar(); entrada.focus(); },
    async criar(apelido) {
      apelidoDaConversa = apelido;
      vista.limpar(); linkDeEntrada = null; conviteNoServico = null; preparoDoConvite = null;
      criandoLink = true;
      try {
        const atual = await obterServico();
        const convite = await atual.criarConvite({ apelido });
        if (atual === servico) vista.convite(convite);
      } finally { criandoLink = false; }
    },
    async chamar(apelido) {
      apelidoDaConversa = apelido;
      abrirSaidaNoGesto();
      const vez = geracao, preparo = preparoDoConvite;
      // O clique conserva o gesto para a saída, mas o convite só pode ser
      // usado depois da recepção inicial e da leitura do link terminarem.
      if (preparo) await preparo.pronto;
      if (vez !== geracao || preparo !== preparoDoConvite) return;
      const atual = await obterServico();
      if (vez !== geracao || preparo !== preparoDoConvite) return;
      if (atual !== conviteNoServico && linkDeEntrada && atual.ler().estado === "livre") {
        atual.abrirConvite(linkDeEntrada); conviteNoServico = atual;
      }
      if (atual !== servico) return;
      vista.limpar();
      return atual.chamar({ apelido });
    },
    atender: () => { abrirSaidaNoGesto(); return servico?.atender({ apelido: nomeLocal() }); },
    recusar: () => servico?.recusar(),
    encerrar: terminar,
    enviarTexto: texto => {
      if (!servico || fase !== "dentro") throw new Error("Conversa encerrada");
      return servico.enviarTexto(texto);
    },
    async copiar(link) {
      if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(link);
      else {
        const campo = vista.elemento.querySelector(".pessoal-web-link");
        campo.focus(); campo.select();
        vista.falha("Selecione e copie o link.");
      }
    },
    async microfone() {
      if (!voz || !par || fase !== "dentro") throw new Error("Áudio ainda está abrindo");
      if (microfone || abrindoMicrofone) { fecharMicrofone(); return; }
      const estaVoz = voz, estePar = par, vez = geracaoDaMidia;
      abrindoMicrofone = true;
      try {
        // A instância já está preparada; a captura nasce no gesto do botão.
        const captura = estaVoz.ligar();
        // As duas operações têm dono desde o início, inclusive se o IPC falhar
        // enquanto o seletor de microfone ainda estiver aberto.
        void captura.catch(() => {});
        await Promise.all([captura, navegador ? invocarNaPonte("focar_voz", {}) : chamarNativo("focar_voz", {})]);
        if (vez !== geracaoDaMidia || par !== estePar || voz !== estaVoz || fase !== "dentro") {
          if (voz !== estaVoz) await estaVoz.desligar();
          return;
        }
        // Falhas de captura/codificação invalidam a geração no callback;
        // ligar não depende de respostas do medidor do AudioWorklet.
        estaVoz.mudo(false); estaVoz.modo(2);
        estaVoz.entrar(estePar.enlace.transporteDaVoz, estePar.meta.papel === "quem-liga" ? 1 : 2);
        microfone = true; vista.microfone(true);
      } catch (erro) {
        // Invalida uma captura pendente sem desligar a reprodução do par nem
        // atingir uma tentativa mais nova que use a mesma instância de voz.
        if (voz === estaVoz && vez === geracaoDaMidia) fecharMicrofone();
        throw erro;
      } finally { if (voz === estaVoz && vez === geracaoDaMidia) abrindoMicrofone = false; }
    },
    compartilhar: () => {
      if (!tela || fase !== "dentro") throw new Error("Tela ainda está abrindo");
      return tela.noAr ? tela.parar() : tela.compartilhar();
    },
    podeCompartilhar: () => typeof temCapturaParaCodificar === "function" && temCapturaParaCodificar(),
  } });
  document.body.append(vista.elemento);

  function fecharMicrofone() {
    const tinhaCaptura = microfone || abrindoMicrofone;
    ++geracaoDaMidia;
    microfone = false; abrindoMicrofone = false;
    vista.microfone(false);
    // desligar invalida imediatamente uma permissão/getUserMedia pendente.
    if (voz) { voz.mudo(true); if (tinhaCaptura) voz.calarMicrofone(); }
  }
  function soltarMidia(guardarVoz = false) {
    fecharMicrofone();
    if (!guardarVoz) { void voz?.desligar().catch(() => {}); voz = null; preparandoVoz = null; }
    par = null;
    if (tela) tela.fechar();
    tela = null; vista.palco.replaceChildren(); vista.palco.hidden = true;
  }
  async function prepararVoz(atual) {
    if (voz) return voz;
    if (preparandoVoz) return preparandoVoz;
    const vez = geracaoDaMidia;
    const promessa = (async () => {
      const { bytes } = await carregarOWasmDaPagina();
      if (vez !== geracaoDaMidia || atual !== servico) return null;
    const criada = criarVozDoNavegador({ wasm: atual.wasm, bytesDoWasm: bytes, aoAcontecer(evento) {
      if (voz !== criada) return;
      if (evento.tipo === "audio-fechado") { microfone = false; vista.microfone(false); }
      if (evento.tipo === "falha" && ["codificador", "captura", "reprodução"].includes(evento.onde)) {
        fecharMicrofone(); vista.falha("O áudio parou. Tente ligar o microfone novamente.");
      }
    } });
    voz = criada;
      return criada;
    })().finally(() => { if (preparandoVoz === promessa) preparandoVoz = null; });
    preparandoVoz = promessa;
    return promessa;
  }
  function abrirSaidaNoGesto() {
    if (!voz) { vista.falha("O áudio está preparando. Toque no microfone depois de conectar para abrir a escuta."); return; }
    // AudioContext nasce aqui, no gesto, sem solicitar getUserMedia.
    void voz.ligar({ somenteOuvir: true }).catch(erro => vista.falha(`Não foi possível abrir a escuta: ${erro.message || erro}`));
  }
  async function receberPar(enlace, meta, minhaGeracao) {
    soltarMidia(true);
    if (!enlace || minhaGeracao !== geracao) return;
    const atual = servico, vez = geracaoDaMidia;
    par = { enlace, meta };
    // Aceitar a conversa cede a voz do servidor, sem fechar sua sessão/chat.
    await (navegador ? invocarNaPonte("focar_voz", {}) : chamarNativo("focar_voz", {}));
    if (minhaGeracao !== geracao || vez !== geracaoDaMidia || servico !== atual || par?.enlace !== enlace) return;
    const preparada = await prepararVoz(atual);
    if (!preparada || vez !== geracaoDaMidia || par?.enlace !== enlace) return;
    preparada.mudo(true);
    preparada.entrar(enlace.transporteDaVoz, meta.papel === "quem-liga" ? 1 : 2);
    tela = criarCompartilhamentoPessoalWeb({ servico: atual, transporte: enlace.tela,
      palco: vista.palco, enviarControle: controle => atual.enviarControle(controle),
      telaCheiaNativa: navegador ? null : ligada => chamarNativo("tela_cheia", { ligada }),
      aoEstado: (estado, detalhe) => {
        if (estado === "transmitindo" || estado === "parada") vista.compartilhando(estado === "transmitindo");
        if (estado === "erro") vista.falha(detalhe || "Não foi possível abrir o compartilhamento.");
      } });
  }
  function terminar() {
    preparoDoConvite = null;
    soltarMidia();
    if (fase === "recebendo" && !recebendoCodigo) void servico?.revogarConvite().catch(() => {});
    else servico?.encerrar();
    // A caixa pessoal continua alcançável enquanto a página está aberta.
    if (recebendoCodigo && servico) {
      fase = "encerrada"; vista.estado({ estado: fase });
      return;
    }
    // Uma operação de criação ainda em voo não pode instalar um convite depois
    // de sair. Os callbacks da geração antiga deixam de alcançar esta tela.
    ++geracao;
    servico?.destruir(); servico = null; preparando = null;
    fase = "encerrada"; vista.estado({ estado: fase });
    entrada.textContent = "CONVERSAR POR LINK";
  }
  async function obterServico() {
    if (servico) return servico;
    if (preparando) return preparando;
    const vez = geracao;
    const promessa = (async () => {
      const faltam = [["conexão direta", typeof RTCPeerConnection === "function"], ["WebAssembly", typeof WebAssembly === "object"],
        ["codificador de voz", typeof AudioEncoder === "function"], ["decodificador de voz", typeof AudioDecoder === "function"],
        ["saída de áudio", typeof AudioContext === "function" && typeof AudioWorkletNode === "function"],
        ["captura de áudio", typeof navigator.mediaDevices?.getUserMedia === "function"]].filter(([, existe]) => !existe).map(([nome]) => nome);
      if (faltam.length) {
        const erro = new Error(`Este navegador do sistema não oferece ${faltam.join(", ")}. Atualize o sistema ou o componente de navegação para conversar com voz.`);
        erro.capacidadeAusente = true; throw erro;
      }
      const opcoes = navegador ? {} : {
        identidade: { ...(await chamarNativo("pessoal_identidade")), assinar: intencao => chamarNativo("pessoal_assinar", { intencao }) },
        conferirPar: (codigo, publica, apelido = null, direcao = null) => chamarNativo("pessoal_conferir_par", { codigo, publica, apelido, direcao }),
        reservarChamada: (id, ativo = true) => chamarNativo("pessoal_reservar", { id, ativo }),
      };
      return criarServicoDeChamadasWeb({ ...opcoes,
      lerPerfil: (codigo, publica) => typeof lerPerfilPessoalWeb === "function" ? lerPerfilPessoalWeb(codigo, publica) : null,
      async obterPerfil(apelido) {
        let retrato = null;
        try {
          const local = await (navegador ? invocarNaPonte("meu_retrato", {}) : chamarNativo("meu_retrato", {}));
          if (typeof retratoParaChamadaPessoal === "function") retrato = await retratoParaChamadaPessoal(local);
        } catch { /* O retrato é opcional. */ }
        return { apelido, retrato };
      },
      async aoPerfil(perfil) {
        if (vez !== geracao) return;
        if (typeof guardarPerfilPessoalWeb === "function") await guardarPerfilPessoalWeb(perfil);
      },
      aoRecepcao(estado) {
        if (vez !== geracao) return;
        if (estado.estado === "registrada") anunciarSala({ estado: "NoAr", registro: { registro: "Registrado" }, firewall: null });
        else if (estado.estado === "indisponivel") anunciarSala({ estado: "Falhou", motivo: "NaoSubiu" });
        else if (estado.estado === "desligada") anunciarSala({ estado: "Desligada" });
        else anunciarSala({ estado: "Subindo" });
      },
      aoEstado(estado) {
        if (vez !== geracao || (estado.estado === "recebendo" && !criandoLink)) return;
        if (fase === "encerrada" && ["tocando", "chamando"].includes(estado.estado)) vista.limpar();
        fase = estado.estado; vista.estado(estado);
        if (fase === "tocando") vista.abrir({ apelido: nomeLocal() });
        entrada.textContent = viva() ? "VOLTAR À CONVERSA" : "CONVERSAR POR LINK";
        if (fase === "encerrada") { soltarMidia(); if (servico) void prepararVoz(servico).catch(() => {}); }
        if (fase === "tocando" && servico) void prepararVoz(servico).catch(() => {});
      },
      aoTexto(texto) { if (vez === geracao) vista.texto(texto); },
      aoPar(enlace, meta) {
        if (vez !== geracao) { enlace?.abortar(); return; }
        void receberPar(enlace, meta, vez).catch(() => {
          if (vez === geracao) { soltarMidia(); vista.falha("Não foi possível preparar o áudio ou a tela."); }
        });
      },
      aoControle(controle) { if (vez === geracao) tela?.receberControle(controle); },
    });
    })().then(async criado => {
      if (vez !== geracao) { criado.destruir(); throw new DOMException("Cancelada", "AbortError"); }
      servico = criado;
      await prepararVoz(criado);
      return criado;
    }).finally(() => { if (preparando === promessa) preparando = null; });
    preparando = promessa;
    return promessa;
  }
  async function abrir(link = null) {
    if (viva()) { vista.abrir(); if (link) vista.falha("Encerre a conversa atual antes de abrir outro convite."); return; }
    if (servico && !recebendoCodigo) terminar();
    vista.limpar(); fase = "livre";
    linkDeEntrada = link; conviteNoServico = null; preparoDoConvite = null;
    vista.abrir({ porConvite: Boolean(link), apelido: apelidoDaConversa || document.getElementById("boot-perfil-nome")?.textContent?.replace(/^—$/, "") || "" });
    if (link) {
      const vez = geracao, preparo = { pronto: null };
      preparoDoConvite = preparo;
      preparo.pronto = (async () => {
        const atual = await obterServico();
        await preferenciaInicial;
        if (recebendoCodigo) await atual.receberPorCodigo({ apelido: nomeLocal() });
        if (vez !== geracao || preparo !== preparoDoConvite || atual !== servico) return;
        atual.abrirConvite(link); conviteNoServico = atual;
      })();
      try { await preparo.pronto; }
      catch (erro) { if (vez === geracao && preparo === preparoDoConvite) { if (!viva()) vista.estado({ estado: "encerrada" }); vista.falha((erro?.capacidadeAusente || erro?.origemDoConviteInvalida) ? erro.message : viva() ? "Encerre a ligação atual antes de abrir outro convite." : "Este convite é inválido ou expirou. Peça um novo link."); } }
    }
  }
  function tomarFragmento() {
    const { hash, href } = location;
    if (!hash.startsWith("#chamada/")) return;
    history.replaceState(history.state, "", href.slice(0, -hash.length));
    void abrir(hash);
  }
  const nomeLocal = () => apelidoDaConversa || String(document.getElementById("boot-perfil-nome")?.textContent || "").replace(/^—$/, "").trim() || "Pessoa";
  function anunciarSala(estado) {
    estadoDoCodigoDireto = estado;
    document.dispatchEvent(new CustomEvent("seele:estado-pessoal-direto", { detail: estado }));
    if (navegador) {
      estadoPessoalDaPonte = estado;
      emitirNaPonte("seele://sala", estado);
    }
  }
  window.seelePessoalWeb = Object.freeze({
    get estadoDaSala() { return estadoDoCodigoDireto; },
    async chamarCodigo({ codigo, apelido } = {}) {
      if (apelido) apelidoDaConversa = apelido;
      abrirSaidaNoGesto();
      vista.abrir({ apelido: apelido || nomeLocal() }); vista.limpar();
      try { return await (await obterServico()).chamarCodigo({ codigo, apelido: apelido || nomeLocal() }); }
      catch (erro) {
        // Só ausência explícita permite ao desktop tentar a sala da 0.17.3.
        if (!navegador && (erro.capacidadeAusente || erro.status === 404 || String(erro.message).endsWith("SemRegistro"))) {
          vista.fechar(); return { ausente: true };
        }
        vista.falha(String(erro.message || erro)); throw erro;
      }
    },
    atender: () => { abrirSaidaNoGesto(); return servico?.atender({ apelido: nomeLocal() }); }, recusar: () => servico?.recusar(), encerrar: terminar,
    async receber(receber) {
      const vez = ++geracaoDaRecepcao;
      recebendoCodigo = Boolean(receber);
      // Desliga já, mesmo enquanto a preferência durável ainda é gravada.
      const revogacao = receber ? null : servico?.revogarConvite();
      void revogacao?.catch(() => {});
      anunciarSala({ estado: receber ? "Subindo" : "Desligada" });
      try {
        if (navegador) await preferenciaDeReceberPessoalWeb(receber);
        if (vez !== geracaoDaRecepcao) return;
        if (!receber) {
          await revogacao;
          if (vez === geracaoDaRecepcao) anunciarSala({ estado: "Desligada" });
          return;
        }
        const atual = await obterServico();
        if (vez !== geracaoDaRecepcao) return;
        await atual.receberPorCodigo({ apelido: nomeLocal() });
        if (vez === geracaoDaRecepcao) anunciarSala({ estado: "NoAr", registro: { registro: "Registrado" }, firewall: null });
      } catch (erro) {
        if (vez !== geracaoDaRecepcao) return;
        if (receber) anunciarSala({ estado: "Falhou", motivo: "NaoSubiu" });
        throw erro;
      }
    },
  });
  const preferenciaInicial = navegador ? preferenciaDeReceberPessoalWeb() : chamarNativo("pessoal_identidade").then(i => i.receber);
  void preferenciaInicial.then(receber => window.seelePessoalWeb.receber(receber))
    .catch(erro => { anunciarSala({ estado: "Falhou", motivo: "NaoSubiu" }); entrada.title = String(erro.message || erro); console.warn("receber pelo código:", erro); });
  if (!navegador) window.__TAURI__.event.listen("seele://preferencia-pessoal", ({ payload }) => {
    void window.seelePessoalWeb.receber(Boolean(payload)).catch(erro => vista.falha(String(erro.message || erro)));
  });

  const cederAudio = () => { fecharMicrofone(); void voz?.desligar().catch(() => {}); };
  document.addEventListener("seele:foco-servidor", cederAudio);
  if (!navegador) window.__TAURI__.event.listen("seele://foco-pessoal", cederAudio);
  entrada.hidden = false;
  entrada.addEventListener("click", () => void abrir());
  if (typeof CELULAR === "object" && CELULAR) CELULAR.registrar("conversaPessoal", { abrir });
  window.addEventListener("hashchange", tomarFragmento);
  window.addEventListener("pagehide", () => { ++geracaoDaRecepcao; recebendoCodigo = false; terminar(); });
  document.addEventListener("freeze", () => { ++geracaoDaRecepcao; recebendoCodigo = false; terminar(); });
  const retomarRecepcao = () => {
    if (servico || preparando) return;
    void (navegador ? preferenciaDeReceberPessoalWeb() : chamarNativo("pessoal_identidade").then(i => i.receber))
      .then(receber => window.seelePessoalWeb.receber(receber)).catch(erro => vista.falha(String(erro.message || erro)));
  };
  window.addEventListener("pageshow", retomarRecepcao);
  document.addEventListener("resume", retomarRecepcao);
  tomarFragmento();
}

if (typeof PONTE_NO_LUGAR_DO_TAURI !== "undefined") {
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", instalarConversasPessoaisWeb, { once: true });
  else instalarConversasPessoaisWeb();
}
