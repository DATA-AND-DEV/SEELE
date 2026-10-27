// Integra a conversa por link sem criar uma sessão fictícia de servidor.
"use strict";

function instalarConversasPessoaisWeb() {
  if (!PONTE_NO_LUGAR_DO_TAURI) return;
  const entrada = document.getElementById("boot-conversa-pessoal");
  if (!entrada) return;
  let servico = null, preparando = null, voz = null, tela = null, par = null;
  let geracao = 0, geracaoDaMidia = 0, microfone = false, abrindoMicrofone = false;
  let fase = "livre", linkDeEntrada = null, conviteNoServico = null;
  const viva = () => !["livre", "encerrada"].includes(fase);
  const vista = criarVistaDaConversaPessoal({ acoes: {
    voltar() { terminar(); vista.fechar(); entrada.focus(); },
    async criar(apelido) {
      vista.limpar(); linkDeEntrada = null; conviteNoServico = null;
      const atual = await obterServico();
      const convite = await atual.criarConvite({ apelido });
      if (atual === servico) vista.convite(convite);
    },
    async chamar(apelido) {
      const atual = await obterServico();
      if (atual !== conviteNoServico && linkDeEntrada && atual.ler().estado === "livre") {
        atual.abrirConvite(linkDeEntrada); conviteNoServico = atual;
      }
      if (atual !== servico) return;
      vista.limpar();
      return atual.chamar({ apelido });
    },
    atender: () => servico?.atender(),
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
        await estaVoz.ligar();
        if (vez !== geracaoDaMidia || par !== estePar || voz !== estaVoz || fase !== "dentro") {
          await estaVoz.desligar(); return;
        }
        // Falhas de captura/codificação invalidam a geração no callback;
        // ligar não depende de respostas do medidor do AudioWorklet.
        estaVoz.modo(2);
        estaVoz.entrar(estePar.enlace.transporteDaVoz, estePar.meta.papel === "quem-liga" ? 1 : 2);
        microfone = true; vista.microfone(true);
      } finally { if (voz === estaVoz) abrindoMicrofone = false; }
    },
    compartilhar: () => {
      if (!tela || fase !== "dentro") throw new Error("Tela ainda está abrindo");
      return tela.noAr ? tela.parar() : tela.compartilhar();
    },
    podeCompartilhar: () => typeof temCapturaParaCodificar === "function" && temCapturaParaCodificar(),
  } });
  document.body.append(vista.elemento);

  function fecharMicrofone() {
    ++geracaoDaMidia;
    microfone = false; abrindoMicrofone = false;
    vista.microfone(false);
    // desligar invalida imediatamente uma permissão/getUserMedia pendente.
    if (voz) void voz.desligar().catch(() => {});
  }
  function soltarMidia() {
    fecharMicrofone();
    voz = null; par = null;
    if (tela) tela.fechar();
    tela = null; vista.palco.replaceChildren(); vista.palco.hidden = true;
  }
  async function receberPar(enlace, meta, minhaGeracao) {
    soltarMidia();
    if (!enlace || minhaGeracao !== geracao) return;
    const atual = servico, vez = geracaoDaMidia;
    par = { enlace, meta };
    // Aceitar a conversa cede a voz do servidor, sem fechar sua sessão/chat.
    await invocarNaPonte("focar_voz", {});
    const { bytes } = await carregarOWasmDaPagina();
    if (minhaGeracao !== geracao || vez !== geracaoDaMidia || servico !== atual || par?.enlace !== enlace) return;
    const criada = criarVozDoNavegador({ wasm: atual.wasm, bytesDoWasm: bytes, aoAcontecer(evento) {
      if (voz !== criada) return;
      if (evento.tipo === "audio-fechado") { microfone = false; vista.microfone(false); }
      if (evento.tipo === "falha" && ["codificador", "captura", "reprodução"].includes(evento.onde)) {
        fecharMicrofone(); vista.falha("O áudio parou. Tente ligar o microfone novamente.");
      }
    } });
    voz = criada;
    tela = criarCompartilhamentoPessoalWeb({ servico: atual, transporte: enlace.tela,
      palco: vista.palco, enviarControle: controle => atual.enviarControle(controle),
      aoEstado: (estado, detalhe) => {
        if (estado === "transmitindo" || estado === "parada") vista.compartilhando(estado === "transmitindo");
        if (estado === "erro") vista.falha(detalhe || "Não foi possível abrir o compartilhamento.");
      } });
  }
  function terminar() {
    soltarMidia();
    if (fase === "recebendo") void servico?.revogarConvite().catch(() => {});
    else servico?.encerrar();
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
    const promessa = criarServicoDeChamadasWeb({
      aoEstado(estado) {
        if (vez !== geracao) return;
        if (fase === "encerrada" && ["tocando", "chamando"].includes(estado.estado)) vista.limpar();
        fase = estado.estado; vista.estado(estado);
        entrada.textContent = viva() ? "VOLTAR À CONVERSA" : "CONVERSAR POR LINK";
        if (fase === "encerrada") soltarMidia();
      },
      aoTexto(texto) { if (vez === geracao) vista.texto(texto); },
      aoPar(enlace, meta) {
        if (vez !== geracao) { enlace?.abortar(); return; }
        void receberPar(enlace, meta, vez).catch(() => {
          if (vez === geracao) { soltarMidia(); vista.falha("Não foi possível preparar o áudio ou a tela."); }
        });
      },
      aoControle(controle) { if (vez === geracao) tela?.receberControle(controle); },
    }).then(criado => {
      if (vez !== geracao) { criado.destruir(); throw new DOMException("Cancelada", "AbortError"); }
      servico = criado;
      return criado;
    }).finally(() => { if (preparando === promessa) preparando = null; });
    preparando = promessa;
    return promessa;
  }
  async function abrir(link = null) {
    if (viva()) { vista.abrir(); if (link) vista.falha("Encerre a conversa atual antes de abrir outro convite."); return; }
    terminar(); vista.limpar(); fase = "livre";
    linkDeEntrada = link; conviteNoServico = null;
    vista.abrir({ porConvite: Boolean(link), apelido: document.getElementById("boot-perfil-nome")?.textContent?.replace(/^—$/, "") ?? "" });
    if (link) {
      const vez = geracao;
      try { const atual = await obterServico(); atual.abrirConvite(link); if (atual === servico) conviteNoServico = atual; }
      catch { if (vez === geracao) { vista.estado({ estado: "encerrada" }); vista.falha("Este convite é inválido ou expirou. Peça um novo link."); } }
    }
  }
  function tomarFragmento() {
    const { hash, href } = location;
    if (!hash.startsWith("#chamada/")) return;
    history.replaceState(history.state, "", href.slice(0, -hash.length));
    void abrir(href);
  }
  entrada.hidden = false;
  entrada.addEventListener("click", () => void abrir());
  if (typeof CELULAR === "object" && CELULAR) CELULAR.registrar("conversaPessoal", { abrir });
  window.addEventListener("hashchange", tomarFragmento);
  window.addEventListener("pagehide", terminar);
  document.addEventListener("freeze", terminar);
  tomarFragmento();
}

if (typeof PONTE_NO_LUGAR_DO_TAURI !== "undefined" && PONTE_NO_LUGAR_DO_TAURI) {
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", instalarConversasPessoaisWeb, { once: true });
  else instalarConversasPessoaisWeb();
}
