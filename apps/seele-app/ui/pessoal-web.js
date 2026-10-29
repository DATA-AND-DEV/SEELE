// Vista da conversa pessoal. As ações vêm do serviço autenticado; esta tela
// não cria identidades, não interpreta SDP e não imita um Snapshot de servidor.
"use strict";

function criarVistaDaConversaPessoal({ acoes, documento = document }) {
  const no = (tag, classe, texto = "") => {
    const el = documento.createElement(tag);
    el.className = classe;
    el.textContent = texto;
    return el;
  };
  const desenhos = { VOLTAR: "recuar", "ENTRAR NA CONVERSA": "avancar", "CRIAR CONVERSA": "avancar", "COPIAR LINK": "anexo", ATENDER: "fone", RECUSAR: "sair", ENVIAR: "avancar", "LIGAR MICROFONE": "microfone", "DESLIGAR MICROFONE": "microfone", "COMPARTILHAR TELA": "aparencia", "PARAR COMPARTILHAMENTO": "aparencia", "SAIR DA CONVERSA": "sair", CANCELAR: "sair", "ENCERRAR CONVITE": "sair" };
  const rotular = (el, texto) => {
    if (el.getAttribute("aria-label") === texto && el.querySelector(".glifo")) return;
    el.setAttribute("aria-label", texto);
    const icone = texto === "LIGAR MICROFONE" ? glifoComEstado("microfone", true) : glifo(desenhos[texto] || "avancar");
    const curto = { "LIGAR MICROFONE": "MICROFONE", "DESLIGAR MICROFONE": "MICROFONE", "COMPARTILHAR TELA": "COMPARTILHAR", "PARAR COMPARTILHAMENTO": "PARAR TELA", "SAIR DA CONVERSA": "SAIR" };
    el.replaceChildren(icone, no("span", "pessoal-web-botao-texto", curto[texto] || texto));
  };
  const botao = (texto, acao, principal = false) => {
    const el = no("button", principal ? "botao" : "botao-fantasma", texto);
    el.type = "button";
    rotular(el, texto);
    el.addEventListener("click", () => executar(acao));
    return el;
  };
  const raiz = no("section", "pessoal-web");
  raiz.id = "conversa-pessoal-web";
  raiz.hidden = true;
  raiz.tabIndex = -1;
  raiz.setAttribute("aria-label", "Conversa pessoal");
  const topo = no("header", "pessoal-web-topo");
  const titulo = no("h1", "pessoal-web-titulo", "Conversa pessoal");
  const voltar = botao("VOLTAR", () => acoes.voltar());
  const assinatura = no("div", "pessoal-web-assinatura");
  const marca = no("img", "pessoal-web-marca");
  marca.src = "marca-assinatura.svg"; marca.alt = "SEELE";
  assinatura.append(marca, titulo);
  topo.append(assinatura, voltar);
  const corpo = no("div", "pessoal-web-corpo");
  const palco = no("div", "pessoal-web-palco");
  const centro = no("div", "pessoal-web-centro");
  const estado = no("p", "pessoal-web-estado", "Converse por um link.");
  estado.setAttribute("role", "status");
  const nomeRotulo = no("label", "pessoal-web-nome", "Seu nome");
  const nome = no("input", "");
  nome.autocomplete = "nickname";
  nome.required = true;
  nomeRotulo.append(nome);
  const entrar = botao("ENTRAR NA CONVERSA", () => {
    if (!nome.reportValidity()) return;
    return acoes.chamar(nome.value.trim());
  }, true);
  const criar = botao("CRIAR CONVERSA", () => {
    if (!nome.reportValidity()) return;
    return acoes.criar(nome.value.trim());
  }, true);
  const convite = no("div", "pessoal-web-convite");
  const link = no("input", "pessoal-web-link");
  link.readOnly = true;
  link.setAttribute("aria-label", "Link da conversa");
  const copiar = botao("COPIAR LINK", () => acoes.copiar(link.value), true);
  convite.append(link, copiar);
  const atender = botao("ATENDER", () => acoes.atender(), true);
  const recusar = botao("RECUSAR", () => acoes.recusar());
  const conviteAcoes = no("div", "pessoal-web-acoes");
  conviteAcoes.append(criar, entrar, atender, recusar);
  const participantes = no("div", "pessoal-web-participantes");
  participantes.setAttribute("aria-label", "Participantes");
  function cartao() {
    const el = no("div", "pessoal-web-participante");
    const inicial = no("span", "pessoal-web-inicial"); inicial.setAttribute("aria-hidden", "true");
    const nome = no("strong", "pessoal-web-participante-nome");
    const papel = no("span", "pessoal-web-participante-papel");
    el.append(inicial, nome, papel); participantes.append(el);
    return { el, inicial, nome, papel };
  }
  const meuCartao = cartao(), outroCartao = cartao();
  const entradaTitulo = no("h2", "pessoal-web-entrada-titulo", "Conversa pessoal");
  centro.append(entradaTitulo, estado, participantes, nomeRotulo, convite, conviteAcoes);
  const tela = no("div", "pessoal-web-tela");
  tela.setAttribute("aria-label", "Tela compartilhada");
  tela.hidden = true;
  palco.append(centro, tela);
  const chat = no("section", "pessoal-web-chat");
  chat.setAttribute("aria-label", "Chat da conversa");
  const mensagens = no("ol", "pessoal-web-mensagens");
  mensagens.setAttribute("role", "log");
  mensagens.setAttribute("aria-label", "Mensagens");
  mensagens.setAttribute("aria-relevant", "additions");
  const form = no("form", "pessoal-web-escrever");
  const texto = no("textarea", "pessoal-web-texto");
  texto.rows = 2;
  texto.maxLength = 2000;
  texto.placeholder = "Escreva uma mensagem";
  texto.setAttribute("aria-label", "Mensagem");
  const enviar = no("button", "botao-fantasma", "ENVIAR");
  enviar.type = "submit";
  rotular(enviar, "ENVIAR");
  form.append(texto, enviar);
  form.addEventListener("submit", event => {
    event.preventDefault();
    if (!texto.value.trim() || enviar.disabled) return;
    const rascunho = texto.value;
    const conversa = geracaoDaConversa;
    executar(async () => {
      await acoes.enviarTexto(rascunho);
      if (conversa === geracaoDaConversa && texto.value === rascunho) texto.value = "";
    });
  });
  texto.addEventListener("keydown", event => {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      form.requestSubmit();
    }
  });
  const chatTitulo = no("h2", "pessoal-web-chat-titulo", "MENSAGENS");
  chatTitulo.prepend(glifo("linhas"));
  chat.append(chatTitulo, mensagens, form);
  corpo.append(palco, chat);
  const rodape = no("footer", "pessoal-web-rodape");
  const mic = botao("LIGAR MICROFONE", () => acoes.microfone());
  mic.setAttribute("aria-pressed", "false");
  const compartilhar = botao("COMPARTILHAR TELA", () => acoes.compartilhar());
  const desligar = botao("SAIR DA CONVERSA", () => acoes.encerrar());
  const erro = no("p", "pessoal-web-erro");
  erro.setAttribute("role", "alert");
  erro.hidden = true;
  rodape.append(mic, compartilhar, desligar);
  raiz.append(topo, corpo, erro, rodape);
  let retrato = { estado: "livre" };
  let recebido = false;
  let nomeDoPar = "";
  let ocupado = false;
  let operacao = 0;
  let geracaoDaConversa = 0;
  const ids = new Set();
  function limparHistorico() {
    ++geracaoDaConversa;
    nomeDoPar = ""; ids.clear(); mensagens.replaceChildren(); texto.value = "";
  }
  const frases = {
    livre: "Nova conversa.", recebendo: "Aguardando chamada.",
    procurando: "Procurando pelo código…", chamando: "Chamando…", tocando: "Quer entrar na conversa.",
    conectando: "Conectando…", reconectando: "Reconectando…",
    dentro: "Conversa conectada", encerrada: "Conversa encerrada",
  };
  function desenhar() {
    const fase = retrato.estado;
    const dentro = fase === "dentro";
    entradaTitulo.hidden = !["livre", "encerrada", "recebendo"].includes(fase);
    raiz.dataset.fase = fase;
    const iniciais = texto => texto.trim().split(/\s+/u).filter(Boolean).slice(0, 2).map(p => Array.from(p)[0]).join("").toUpperCase();
    meuCartao.nome.textContent = nome.value.trim() || "Você";
    meuCartao.inicial.textContent = iniciais(nome.value) || "—";
    meuCartao.papel.textContent = "VOCÊ";
    outroCartao.nome.textContent = nomeDoPar;
    outroCartao.inicial.textContent = iniciais(nomeDoPar);
    outroCartao.papel.textContent = dentro ? "NA CONVERSA" : fase === "tocando" ? "QUER ENTRAR" : "CONECTANDO";
    outroCartao.el.hidden = !nomeDoPar;
    participantes.hidden = !["dentro", "conectando", "reconectando", "tocando"].includes(fase);
    estado.textContent = fase === "tocando" && retrato.apelido
      ? `${retrato.apelido} está chamando.` : frases[fase] ?? "Aguardando…";
    const podeEntrar = fase === "livre" || fase === "encerrada";
    nomeRotulo.hidden = !podeEntrar;
    criar.hidden = !podeEntrar || recebido;
    entrar.hidden = !podeEntrar || !recebido;
    convite.hidden = fase !== "recebendo" || !link.value;
    atender.hidden = recusar.hidden = fase !== "tocando";
    mic.hidden = compartilhar.hidden = !dentro;
    compartilhar.disabled = ocupado || !acoes.podeCompartilhar();
    compartilhar.title = acoes.podeCompartilhar() ? "Compartilhar tela sem som do sistema" : "Captura de tela indisponível";
    desligar.hidden = ["livre", "encerrada"].includes(fase);
    rotular(desligar, fase === "recebendo" ? "ENCERRAR CONVITE"
      : dentro ? "SAIR DA CONVERSA" : "CANCELAR");
    enviar.disabled = texto.disabled = !dentro || ocupado;
    for (const item of [criar, entrar, atender, recusar, copiar, mic]) item.disabled = ocupado;
    // Sair e voltar continuam disponíveis durante operações demoradas.
    chat.hidden = !dentro;
  }
  async function executar(acao) {
    const minha = ++operacao;
    ocupado = true;
    erro.hidden = true;
    desenhar();
    try { await acao(); }
    catch (erro) { if (minha === operacao) falha((erro?.capacidadeAusente === true || erro?.origemDoConviteInvalida === true) ? erro.message : "Não foi possível concluir. Tente novamente."); }
    finally { if (minha === operacao) { ocupado = false; desenhar(); } }
  }
  function falha(frase) {
    erro.textContent = frase;
    erro.hidden = false;
  }
  desenhar();
  return Object.freeze({
    elemento: raiz,
    palco: tela,
    abrir({ porConvite = false, apelido = "" } = {}) {
      recebido = porConvite;
      if (apelido.trim()) nome.value = apelido;
      raiz.hidden = false;
      desenhar();
      (retrato.estado === "livre" ? nome : raiz).focus();
    },
    estado(novo) {
      if (novo.estado === "encerrada") {
        ++operacao; ocupado = false;
        limparHistorico();
      }
      retrato = novo;
      if (typeof novo.apelido === "string") nomeDoPar = novo.apelido;
      desenhar();
    },
    convite({ link: valor }) { link.value = valor; desenhar(); },
    texto({ id, de, texto: conteudo }) {
      if (retrato.estado !== "dentro") return;
      const chave = `${de}:${id}`;
      if (ids.has(chave)) return;
      ids.add(chave);
      const item = no("li", "pessoal-web-mensagem");
      item.append(no("span", "pessoal-web-autor", de === "eu" ? "Você" : retrato.apelido || "Outra pessoa"),
        no("p", "pessoal-web-conteudo", conteudo));
      mensagens.append(item);
      // A conversa não grava histórico ilimitado na memória da aba.
      if (mensagens.childElementCount > 200) { mensagens.firstElementChild.remove(); ids.delete(ids.values().next().value); }
      mensagens.scrollTop = mensagens.scrollHeight;
      desenhar();
    },
    microfone(ligado) {
      rotular(mic, ligado ? "DESLIGAR MICROFONE" : "LIGAR MICROFONE");
      mic.setAttribute("aria-pressed", String(ligado));
    },
    compartilhando(ligado) { rotular(compartilhar, ligado ? "PARAR COMPARTILHAMENTO" : "COMPARTILHAR TELA"); compartilhar.setAttribute("aria-pressed", String(ligado)); },
    falha,
    limpar({ preservarOperacao = false } = {}) {
      // Criar/entrar limpam a conversa dentro da ação já em andamento.
      // Preservar seu dono mantém o botão bloqueado e a falha visível.
      if (!preservarOperacao) { ++operacao; ocupado = false; }
      retrato = { estado: "livre" };
      limparHistorico(); link.value = "";
      tela.replaceChildren(); tela.hidden = true; erro.hidden = true;
      rotular(mic, "LIGAR MICROFONE"); mic.setAttribute("aria-pressed", "false");
      rotular(compartilhar, "COMPARTILHAR TELA"); compartilhar.setAttribute("aria-pressed", "false");
      desenhar();
    },
    fechar() { ++operacao; ocupado = false; raiz.hidden = true; link.value = ""; },
  });
}
