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

  // As frases são do `frases.js` (P10): o `tests/celular.rs` confere que este
  // arquivo não escreve nenhuma.
  const PALAVRAS = FRASES_DA_LARGA;

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
      h("ul", {}, Object.values(PALAVRAS.itens).map(([nome, texto, comLink]) => h("li", {},
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

  // A tela cheia do navegador sai pelo Esc sem a página receber a tecla (achado
  // da F4): o ouvinte de Escape do `palco-imagem.js` não roda, e o cinema ficava
  // ligado numa janela que já não está em tela cheia. Quando o documento sai da
  // tela cheia por fora, o cinema sai junto, pelo caminho do desktop. A saída
  // pelo botão apaga o cinema antes, e não passa por aqui duas vezes.
  document.addEventListener("fullscreenchange", () => {
    if (document.fullscreenElement || document.body.dataset.cinema !== "sim") return;
    trocarCinema(false).catch((falha) => console.warn("trocarCinema:", falha));
  });

  // O app bloqueia o F5; o navegador não deixa, e recarregar derruba a sessão.
  // Com a sessão aberta, a página pergunta antes de sair.
  window.addEventListener("beforeunload", (evento) => {
    if (document.getElementById("tela-sessao").hidden) return;
    // **Esquecer antes de segurar** (o guarda do lote R, em `tests/frontend.rs`).
    // Pedir confirmação mantém a página viva se a pessoa ficar, e a credencial
    // de uma tentativa, que só existe na memória da página, não pode sobreviver
    // a um fechar que a pessoa começou. Com a sessão aberta ela já foi esquecida
    // ao entrar; isto cobre o resto.
    fecharAceiteDeMods();
    evento.preventDefault();
    evento.returnValue = "";
  });
})();
