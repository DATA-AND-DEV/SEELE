// SEELE — a saída do cinema (a «tela cheia») no celular.
//
// O palco tenta a API nativa disponível; sem ela, amplia dentro da página e
// explica o limite. Aqui está uma saída que fica sempre à vista: um botão,
// preso à safe-area, de alvo de toque inteiro (`celular-sessao.css`).
//
// A página não força orientação. O modo ampliado acompanha a rotação com
// 100dvh/safe-area. Quando o iPhone exige metadata antes do gesto nativo,
// esta faixa oferece o segundo gesto; sem suporte, explica o limite.
//
// O botão nasce quando o cinema liga (`body[data-cinema]`) e some quando ele
// desliga, por qualquer caminho (o botão da cabeça, o duplo toque, a
// transmissão que acabou). Ele é filho do `body` e nasce DEPOIS de o cinema
// esconder os irmãos do caminho do canvas, então o cinema não o esconde. Ao
// sair, o foco volta ao botão de tela cheia.

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "celular") return;

  const { h } = CELULAR;
  const PALAVRAS = FRASES_DO_CELULAR.cinema;
  let saida = null;
  let dica = null;

  function mostrar() {
    if (!saida) {
      saida = h("button", { id: "palco-sair-da-cheia", class: "palco-sair-da-cheia", type: "button", onclick: () => sair() }, PALAVRAS.sair);
      dica = h("p", { id: "palco-dica-girar", class: "palco-dica-girar" }, PALAVRAS.girar);
      document.body.append(saida, dica);
    }
    const modo = document.body.dataset.cinemaModo;
    if (!modo) return;
    saida.textContent = modo === "nativa" ? PALAVRAS.sair : PALAVRAS.voltar;
    dica.textContent = modo === "nativa" ? PALAVRAS.girar
      : modo === "preparando-video" ? PALAVRAS.preparando
      : modo === "video-pronto" ? "" : PALAVRAS.indisponivel;
    // Esta ação precisa de outro gesto: no iPhone a metadata deve existir
    // ANTES de webkitEnterFullscreen, e esperar por ela perderia o gesto.
    if (modo === "video-pronto") {
      const abrir = h("button", { id: "palco-abrir-nativa", class: "botao", type: "button", onclick: () => trocarCinema(true) }, PALAVRAS.abrir);
      abrir.style.pointerEvents = "auto";
      dica.append(abrir);
    }
    // A explicação e o segundo gesto também são necessários em paisagem.
    dica.style.display = modo === "nativa" ? "" : "block";
  }

  function esconder() {
    if (!saida) return;
    saida.remove();
    dica?.remove();
    saida = null;
    dica = null;
  }

  async function sair() {
    await trocarCinema(false);
    esconder();
    $("palco-cheia")?.focus();
  }

  new MutationObserver(() => {
    if (document.body.dataset.cinema === "sim") mostrar();
    else esconder();
  }).observe(document.body, { attributes: true, attributeFilter: ["data-cinema", "data-cinema-modo"] });
})();
