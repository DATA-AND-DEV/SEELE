// SEELE — a aba AJUSTES do celular (F8).
//
// Spec: docs/superpowers/specs/2026-10-09-0.16-celular-design.md, §2.2.
//
// AJUSTES é a tela de CONFIGURAÇÕES do desktop (`#tela-server`, de
// tela-server.js), aberta pelo `abrirServer` de sempre e mostrada como páginas
// da aba: a lista de seções, ou uma seção com «‹ AJUSTES». Ela mora entre o
// topo e a faixa de voz, que continuam vivos: o MUDO e as outras abas ficam a um
// toque. Sair da aba fecha a tela pelo `fecharServer` de sempre.

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "celular") return;

  const { h } = CELULAR;
  const sessao = document.getElementById("tela-sessao");
  const camada = document.getElementById("tela-server");

  // Vão para `FRASES_DO_CELULAR`, no `frases.js`, com o P10 (Tarefa 14).
  const PALAVRAS = Object.freeze({
    ajustes: "AJUSTES",
    voltarAosAjustes: "Voltar à lista de AJUSTES",
  });

  // A camada mora entre o topo e o que estiver embaixo (os avisos, a faixa e as
  // abas): as medidas vêm da moldura, e acompanham quando ela muda.
  function encaixar() {
    const topo = sessao.querySelector(".cel-topo").getBoundingClientRect().bottom;
    const pe = [...sessao.querySelectorAll(":scope > #veredito, :scope > #aviso-fone, :scope > #aviso-audio, :scope > .voz-celular, :scope > .cel-abas")]
      .filter((e) => !e.hidden && getComputedStyle(e).display !== "none")
      .reduce((menor, e) => Math.min(menor, e.getBoundingClientRect().top), innerHeight);
    camada.style.setProperty("--cel-ajustes-topo", `${topo}px`);
    camada.style.setProperty("--cel-ajustes-pe", `${innerHeight - pe}px`);
  }
  const observador = new ResizeObserver(encaixar);
  for (const parte of sessao.querySelectorAll(":scope > .cel-topo, :scope > .voz-celular, :scope > .cel-abas")) {
    observador.observe(parte);
  }
  addEventListener("resize", encaixar);

  // Entrar na aba abre a tela, e sair dela a fecha.
  new MutationObserver(() => {
    const naAba = sessao.dataset.aba === "ajustes";
    if (naAba && camada.hidden) {
      camada.setAttribute("data-cel-pagina", "lista");
      encaixar();
      abrirServer("tela-sessao");
    } else if (!naAba && !camada.hidden) {
      fecharServer();
    }
  }).observe(sessao, { attributes: true, attributeFilter: ["data-aba"] });

  // A tela fechada por outro caminho (o Escape do desktop) devolve a pessoa à
  // conversa, e não a uma aba vazia.
  new MutationObserver(() => {
    if (camada.hidden && sessao.dataset.aba === "ajustes") CELULAR.ajudante("abrirAba")("conversa");
  }).observe(camada, { attributes: true, attributeFilter: ["hidden"] });

  // As páginas: tocar uma seção a mostra sozinha; «‹ AJUSTES» volta à lista.
  camada.querySelector(".server-faixa").prepend(h("button", {
    class: "cel-ajustes-voltar",
    type: "button",
    "aria-label": PALAVRAS.voltarAosAjustes,
    onclick: () => camada.setAttribute("data-cel-pagina", "lista"),
  }, glifo("recuar"), PALAVRAS.ajustes));
  camada.addEventListener("click", (evento) => {
    if (evento.target.closest(".server-coluna .server-secao")) camada.setAttribute("data-cel-pagina", "secao");
  });
})();
