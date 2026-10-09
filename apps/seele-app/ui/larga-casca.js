// SEELE — a casca larga: o desktop no navegador de um computador (F8, D-web-3).
//
// Spec: docs/superpowers/specs/2026-10-09-0.16-celular-design.md, §2.8.
//
// A casca de desktop de hoje, sem o que é do Tauri. O que só o app instalado
// faz (hospedar, guardar versões, atualizar o app, o caminho entre pares) vira
// frase no lugar do controle, sem tirar o controle do DOM: o desktop continua
// desenhando, e a larga só o cobre e escreve a frase. Tudo aqui mora numa IIFE
// que volta sem a marca `larga` (G4), e nada chama o `invoke` (G5).

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "larga") return;

  // Para onde vai quem quer o app do SEELE (decisão da orquestradora, 09/10).
  // Vai para o `frases.js` com o P10, ao lado das frases.
  const PAGINA_DO_APP = "https://seele.app.br/#baixar";

  // Vão para o `frases.js`, com o P10 (Tarefa 14). As do grupo SÓ NO APP DO
  // SEELE são as da lista canônica da matriz, as mesmas que o celular diz.
  const PALAVRAS = Object.freeze({
    proposito: "Voz, vídeo e texto entre pessoas, sem servidor de ninguém no meio. Entre no servidor de alguém pelo link que a pessoa mandou.",
    hospedar: "Hospedar um servidor é pelo app do SEELE. ",
    baixar: "BAIXAR O APP DO SEELE",
    atualizacao: "A página vem da origem: uma versão nova chega ao abrir a página de novo.",
    versoes: "O navegador abre a página da versão do servidor que o link pede. Guardar e abrir outras versões é do app do SEELE.",
    malha: "No navegador, a tela de quem transmite vem sempre pelo servidor, e este computador não a repassa a ninguém.",
    tecla: "No navegador, a tecla de falar só vale com esta página em foco: o navegador não tem atalho para o computador inteiro.",
    comoAbre: "COMO ESTA PÁGINA ABRE",
    comoAbreSub: "Como computador ou como celular, na próxima vez que ela abrir.",
    abriuComoComputador: "Esta página abriu como computador.",
    abrirComoCelular: "ABRIR COMO CELULAR",
    pelaTela: "PELO TAMANHO DA TELA",
    valeNaProxima: "Vale na próxima vez que a página abrir. Abrir de novo agora sai da sessão.",
    naoGuardou: "Este navegador não deixou guardar a escolha: a página continua abrindo pelo tamanho da tela.",
    foraTitulo: "O QUE O NAVEGADOR NÃO FAZ",
    foraSub: "O que fica no app do SEELE, instalado num computador.",
    grupo: "SÓ NO APP DO SEELE",
    itens: [
      ["HOSPEDAR UM SERVIDOR", "Pôr um servidor no ar, o link de convite, a porta e o firewall ficam no app do SEELE, instalado num computador.", true],
      ["A PORTARIA DO SEU SERVIDOR", "Aprovar quem bate, a senha e os convites ficam no app do SEELE que hospeda o servidor."],
      ["INSTALAR E LIGAR MODS", "Os MODs deste servidor rodam aqui. Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE."],
      ["PACOTES DE MOD", "O espaço que os pacotes ocupam, as revisões guardadas e o APAGAR ficam no app do SEELE."],
      ["LIGAR PELO CÓDIGO", "Ligar pelo código é pelo app do SEELE: o navegador não abre o caminho que a ligação usa."],
      ["VERSÕES LADO A LADO", "O navegador abre a página da versão do servidor que o link pede. Guardar e abrir outras versões é do app do SEELE."],
      ["A TELA ENTRE PESSOAS", "No navegador, a tela de quem transmite vem sempre pelo servidor, e este computador não a repassa a ninguém."],
    ],
  });

  /** Um elemento por `createElement`; texto sempre por `textContent`. */
  function h(tag, props = {}, ...filhos) {
    const el = document.createElement(tag);
    for (const [chave, valor] of Object.entries(props)) {
      if (valor === undefined || valor === null || valor === false) continue;
      if (chave === "class") el.className = valor;
      else if (chave === "text") el.textContent = valor;
      else if (chave.startsWith("on")) el.addEventListener(chave.slice(2), valor);
      else el.setAttribute(chave, valor === true ? "" : valor);
    }
    for (const filho of filhos.flat(Infinity)) {
      if (filho === null || filho === undefined || filho === false) continue;
      el.append(typeof filho === "string" ? document.createTextNode(filho) : filho);
    }
    return el;
  }

  const linkDoApp = () => h("a", { href: PAGINA_DO_APP, target: "_blank", rel: "noopener", text: PALAVRAS.baixar });
  const frase = (texto, ...mais) => h("p", { class: "larga-frase" }, texto, ...mais);

  // ------------------------------------------------------------- a entrada

  // O propósito perde a metade «ou ponha o seu no ar a partir desta máquina»,
  // que no navegador seria mentira, e HOSPEDAR AQUI vira a frase do app.
  document.querySelector("#tela-boot .boot-proposito").textContent = PALAVRAS.proposito;
  document.getElementById("botao-hospedar").after(h("p", { class: "nota larga-hospedar" }, PALAVRAS.hospedar, linkDoApp()));

  // -------------------------------------------------------- CONFIGURAÇÕES

  document.getElementById("painel-atualizacao").prepend(frase(PALAVRAS.atualizacao));
  document.getElementById("painel-versoes").prepend(frase(PALAVRAS.versoes));
  document.getElementById("painel-malha").prepend(frase(PALAVRAS.malha));
  document.getElementById("painel-atalhos").prepend(frase(PALAVRAS.tecla));

  // O QUE O NAVEGADOR NÃO FAZ, só com o grupo SÓ NO APP DO SEELE: o que o
  // computador faz também no navegador, aqui ele faz. Uma seção de verdade, que
  // o `abrirSecao` do desktop mostra e esconde com as outras.
  const painel = h("section", { id: "larga-painel-fora", class: "server-painel-secao", hidden: true },
    h("section", { class: "larga-fora-grupo" },
      h("h3", { class: "rotulo", text: PALAVRAS.grupo }),
      h("ul", {}, PALAVRAS.itens.map(([nome, texto, comLink]) => h("li", {},
        h("strong", { text: nome }), h("p", { text: texto }), comLink && linkDoApp())))));
  document.getElementById("tela-server").querySelector(".server-painel").append(painel);
  const botao = h("button", {
    id: "larga-secao-fora",
    class: "server-secao",
    type: "button",
    "data-painel": "larga-painel-fora",
    "data-titulo": PALAVRAS.foraTitulo,
    "data-sub": PALAVRAS.foraSub,
    onclick: () => abrirSecao("larga-secao-fora"),
  }, h("span", { class: "server-secao-icone" }, glifo("desligar")),
    h("span", { class: "server-secao-texto" }, h("span", { class: "server-secao-nome", text: PALAVRAS.foraTitulo })));
  document.getElementById("secao-atualizacao").closest("ul").append(h("li", {}, botao));

  // ---------------------------------------------- como esta página abre
  //
  // A escolha da casca à mão (D-web-3): ela fica guardada neste navegador, e a
  // ponte a lê na próxima carga, antes do critério do aparelho. Nada troca no
  // meio da sessão: trocar de casca é abrir a página de novo.

  const estadoDaEscolha = h("p", { class: "casca-estado nota", role: "status" });
  const pelaTela = h("button", { class: "botao-fantasma", type: "button", "data-casca-escolhida": "auto", hidden: true,
    onclick: () => escolherCasca(null) }, PALAVRAS.pelaTela);
  function guardada() {
    try {
      return localStorage.getItem("seele.casca");
    } catch {
      return null;
    }
  }
  function escolherCasca(casca) {
    try {
      if (casca === null) localStorage.removeItem("seele.casca");
      else localStorage.setItem("seele.casca", casca);
      estadoDaEscolha.textContent = PALAVRAS.valeNaProxima;
    } catch {
      estadoDaEscolha.textContent = PALAVRAS.naoGuardou;
    }
    pelaTela.hidden = guardada() === null;
  }
  pelaTela.hidden = guardada() === null;
  document.getElementById("tela-server").querySelector(".server-painel").append(
    h("section", { id: "larga-painel-casca", class: "server-painel-secao", hidden: true },
      h("p", { class: "larga-frase", text: PALAVRAS.abriuComoComputador }),
      h("button", { class: "botao", type: "button", "data-casca-escolhida": "celular", onclick: () => escolherCasca("celular") },
        PALAVRAS.abrirComoCelular),
      pelaTela,
      estadoDaEscolha));
  document.getElementById("secao-atualizacao").closest("ul").append(h("li", {}, h("button", {
    id: "larga-secao-casca",
    class: "server-secao",
    type: "button",
    "data-painel": "larga-painel-casca",
    "data-titulo": PALAVRAS.comoAbre,
    "data-sub": PALAVRAS.comoAbreSub,
    onclick: () => abrirSecao("larga-secao-casca"),
  }, h("span", { class: "server-secao-icone" }, glifo("atualizar")),
    h("span", { class: "server-secao-texto" }, h("span", { class: "server-secao-nome", text: PALAVRAS.comoAbre })))));

  // ------------------------------------------------------- sair da página

  // O app bloqueia o F5; o navegador não deixa, e recarregar derruba a sessão.
  // Com a sessão aberta, a página pergunta antes de sair.
  window.addEventListener("beforeunload", (evento) => {
    if (document.getElementById("tela-sessao").hidden) return;
    evento.preventDefault();
    evento.returnValue = "";
  });
})();
