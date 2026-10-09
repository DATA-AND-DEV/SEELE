// SEELE — as folhas do celular: o que o desktop pergunta numa caixa, o celular
// pergunta numa folha presa ao pé da tela (F8).
//
// Spec: docs/superpowers/specs/2026-10-09-0.16-celular-design.md, §2.4.
//
// Por enquanto, a folha SENHA DA SALA. O desktop pede a senha pelo
// `CASCA.pedirSenhaDaSala` (P7), que nele é o `window.prompt` de sempre; aqui
// ele abre a folha e devolve a senha, ou `null` quando a pessoa desiste. Uma
// folha de cada vez, sobre um véu que também a fecha. Corte seco: ela aparece,
// não desliza.

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "celular") return;

  const { h } = CELULAR;
  const sessao = document.getElementById("tela-sessao");

  // Vão para `FRASES_DO_CELULAR`, no `frases.js`, com o P10 (Tarefa 14).
  const PALAVRAS = Object.freeze({
    senhaDaSala: "SENHA DA SALA",
    pedeSenha: (sala) => `${sala} pede senha para entrar.`,
    senha: "SENHA",
    entrar: "ENTRAR",
    agoraNao: "AGORA NÃO",
  });

  let fecharAberta = null;

  /**
   * Mostra uma folha e devolve a função que a fecha. Fechar devolve o foco a
   * quem o tinha antes, se esse nó ainda estiver na página.
   */
  function abrirFolha(folha, aoDesistir) {
    fecharAberta?.();
    const antes = document.activeElement;
    const veu = h("div", { class: "cel-veu", onclick: aoDesistir });
    const tecla = (evento) => {
      if (evento.key === "Escape") aoDesistir();
    };
    document.addEventListener("keydown", tecla);
    sessao.append(veu, folha);
    const fechar = () => {
      document.removeEventListener("keydown", tecla);
      veu.remove();
      folha.remove();
      fecharAberta = null;
      if (antes?.isConnected) antes.focus();
    };
    fecharAberta = fechar;
    return fechar;
  }

  function pedirSenhaDaSala(sala) {
    return new Promise((resolver) => {
      let fechar = null;
      const responder = (senha) => {
        fechar();
        resolver(senha);
      };
      const campo = h("input", {
        id: "cel-senha-campo",
        class: "cel-folha-campo",
        type: "password",
        autocomplete: "off",
        autocapitalize: "off",
        spellcheck: "false",
        enterkeyhint: "go",
        onkeydown: (evento) => {
          if (evento.key === "Enter") responder(campo.value);
        },
      });
      const folha = h("section", {
        id: "cel-folha-senha",
        class: "cel-folha",
        role: "dialog",
        "aria-modal": "true",
        "aria-labelledby": "cel-folha-senha-titulo",
      },
        h("header", { class: "cel-folha-cabeca" },
          h("h2", { id: "cel-folha-senha-titulo", class: "cel-folha-titulo", text: PALAVRAS.senhaDaSala })),
        h("div", { class: "cel-folha-corpo" },
          h("p", { class: "cel-folha-nota", text: PALAVRAS.pedeSenha(sala) }),
          h("label", { class: "rotulo", for: "cel-senha-campo", text: PALAVRAS.senha }),
          campo,
          h("div", { class: "cel-folha-acoes" },
            h("button", { class: "botao", type: "button", "data-resposta": "entrar", onclick: () => responder(campo.value) },
              PALAVRAS.entrar),
            h("button", { class: "botao-fantasma", type: "button", "data-resposta": "agora-nao", onclick: () => responder(null) },
              PALAVRAS.agoraNao))));
      fechar = abrirFolha(folha, () => responder(null));
      campo.focus();
    });
  }

  CASCA.pedirSenhaDaSala = pedirSenhaDaSala;
})();
