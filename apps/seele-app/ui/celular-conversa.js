// SEELE — a aba CONVERSA do celular (F8).
//
// Spec: docs/superpowers/specs/2026-10-09-0.16-celular-design.md, §2.2.
//
// A conversa é a do desktop: a lista, a busca, o compositor, os anexos e o
// envio são os do tela-sessao.js. O celular muda o que a mão muda:
//
// - o Return quebra a fileira, e só o ENVIAR envia: o teclado do aparelho não
//   tem Shift+Enter (P8, `CASCA.enterEnvia`);
// - as ações sobre uma mensagem saem do `remover` ao lado da hora e vão para a
//   folha dela, aberta por um toque;
// - o `+` abre a folha ANEXAR;
// - o nome do canal é texto: renomear é pelo SEELE no computador.

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "celular") return;

  const { h } = CELULAR;
  const doDesktop = (id) => document.getElementById(id);

  // As frases são do `frases.js` (P10): o `tests/celular.rs` confere que este
  // arquivo não escreve nenhuma.
  const PALAVRAS = FRASES_DO_CELULAR.conversa;

  CASCA.enterEnvia = false;

  // ----------------------------------------------------------- o compositor

  const campo = doDesktop("campo-mensagem");
  const anexar = h("button", {
    class: "cel-anexar",
    type: "button",
    "aria-label": PALAVRAS.anexar,
    onclick: () => CELULAR.ajudante("folhaDeAnexar")(() => doDesktop("botao-anexar").click()),
  }, glifo("anexo"));
  doDesktop("form-mensagem").prepend(anexar);

  // O texto de exemplo do desktop promete que o Enter envia. Um MOD pode
  // escrever o dele no mesmo campo (a região `compositor`), e esse fica.
  const EXEMPLO_DO_DESKTOP = campo.placeholder;

  // ------------------------------------------------- a folha da mensagem

  doDesktop("lista-mensagens").addEventListener("click", (evento) => {
    // Um link abre o link, e um botão faz o que é dele: só o toque no corpo
    // da mensagem abre a folha.
    if (evento.target.closest("a, button, input, summary, canvas, img")) return;
    const item = evento.target.closest("li.mensagem");
    if (!item) return;
    const indice = Number(item.getAttribute("data-mensagem"));
    const mensagem = mensagens[indice];
    const texto = mensagem?.body ?? item.querySelector(".corpo")?.textContent ?? "";
    const id = mensagem?.id;
    CELULAR.ajudante("folhaDaMensagem")({
      autor: item.querySelector(".mensagem-autor")?.textContent ?? mensagem?.author_nickname ?? "",
      hora: item.querySelector(".mensagem-hora")?.textContent ?? "",
      texto,
      propria: item.classList.contains("propria"),
      podeRemoverDosOutros: podeRemoverMensagem,
      // O REMOVER pelo id da mensagem, na hora do toque. A lista é reconciliada
      // pelo id (lote R63 da F5), e o botão é o mesmo nó de um quadro ao
      // seguinte; mas a mensagem pode ter saído da conversa enquanto a folha
      // estava aberta, e aí não há o que apertar.
      remover: () => doDesktop("lista-mensagens").querySelector(`button[data-moderar-mensagem="${id}"]`)?.click(),
    });
  });

  // ------------------------------------------------------ o nome do canal

  // No desktop, quem administra renomeia pelo nome. No celular o toque diz onde
  // isso se faz. A captura vem antes do ouvinte do desktop, no próprio botão.
  doDesktop("linha-nome").parentElement.addEventListener("click", (evento) => {
    if (!evento.target.closest("#linha-nome")) return;
    evento.stopPropagation();
    evento.preventDefault();
    CELULAR.ajudante("folhaDeFrase")("cel-folha-renomear", PALAVRAS.renomearTitulo, PALAVRAS.salasECanais, PALAVRAS.foraSalas);
  }, true);

  let exemploDoCelular = null;
  CASCA.depoisDeDesenhar.push(() => {
    if (campo.placeholder !== EXEMPLO_DO_DESKTOP && campo.placeholder !== exemploDoCelular) return;
    exemploDoCelular = PALAVRAS.escrever(doDesktop("linha-nome").textContent);
    campo.placeholder = exemploDoCelular;
  });
})();
