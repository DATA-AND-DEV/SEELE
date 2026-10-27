// SEELE — a saída do cinema (a «tela cheia») no celular.
//
// No celular a ponte recusa `tela_cheia` («no celular a tela cheia é o aparelho
// deitado», `ponte-web.js`): o iPhone não tem a Fullscreen API para um
// elemento qualquer, e o cinema do `palco-imagem.js` é só CSS, que esconde tudo
// menos o caminho do canvas. A saída dele era o Esc (que o iPhone não tem) ou o
// duplo toque no canvas. Aqui está uma saída que fica sempre à vista: um botão,
// preso à safe-area, de alvo de toque inteiro (`celular-sessao.css`).
//
// **A página não gira o aparelho.** O cinema daqui é só CSS, sem a tela cheia
// de um elemento, e esta página não chama a API de orientação. Em retrato, uma
// dica discreta (`palco-dica-girar`, só texto, sem pegar toque, no pé e longe
// do SAIR) diz que virar o celular amplia; deitado, o CSS a esconde, e o layout
// acompanha a rotação que a pessoa fizer (o `100dvh` com a safe-area). Visto no
// WebKit do Mac em retrato e em paisagem (a bancada-c), e não num aparelho.
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
    if (saida) return;
    saida = h("button", { id: "palco-sair-da-cheia", class: "palco-sair-da-cheia", type: "button", onclick: () => sair() }, PALAVRAS.sair);
    dica = h("p", { id: "palco-dica-girar", class: "palco-dica-girar" }, PALAVRAS.girar);
    document.body.append(saida, dica);
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
  }).observe(document.body, { attributes: true, attributeFilter: ["data-cinema"] });
})();
