// SEELE — as folhas do celular: o que o desktop pergunta numa caixa ou oferece
// num menu, o celular põe numa folha presa ao pé da tela (F8).
//
// Spec: docs/superpowers/specs/2026-10-09-0.16-celular-design.md, §2.4.
//
// As folhas não agem por conta própria: cada ação aperta o controle do desktop
// que já faz aquilo, ou devolve a resposta a quem perguntou. Uma folha de cada
// vez, sobre um véu que também a fecha, e o Escape fecha. Corte seco: a folha
// aparece, não desliza.
//
// - SENHA DA SALA: o `CASCA.pedirSenhaDaSala` do P7, que no desktop é o
//   `window.prompt` de sempre;
// - a folha de uma mensagem: copiar, apagar a própria, e a frase de remover a
//   dos outros, que é pelo SEELE no computador;
// - ANEXAR: foto ou vídeo, ou arquivo, pelo caminho de arquivo do desktop;
// - a folha de uma frase: o que o celular não faz, dito onde o controle estaria.

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
    fechar: "FECHAR",
    mensagemDe: (quem, hora) => `${quem} · ${hora}`,
    copiar: "COPIAR O TEXTO",
    apagarMinha: "APAGAR A MINHA MENSAGEM",
    remover: "REMOVER",
    foraRemover: "Remover a mensagem de outra pessoa é pelo SEELE no computador.",
    anexar: "ANEXAR",
    midia: "FOTO OU VÍDEO",
    arquivo: "ARQUIVO",
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

  /** O corpo comum de uma folha: o título, e o que ela tem dentro. */
  function folha(id, titulo, ...corpo) {
    return h("section", {
      id,
      class: "cel-folha",
      role: "dialog",
      "aria-modal": "true",
      "aria-labelledby": `${id}-titulo`,
    },
      h("header", { class: "cel-folha-cabeca" },
        h("h2", { id: `${id}-titulo`, class: "cel-folha-titulo", text: titulo })),
      h("div", { class: "cel-folha-corpo" }, ...corpo));
  }

  /** Uma folha só com ações: cada uma fecha a folha e faz o que diz. */
  function folhaDeAcoes(id, titulo, antes, acoes) {
    let fechar = null;
    const botoes = acoes.map(({ texto, dados, classe = "botao-fantasma", fazer }) =>
      h("button", {
        class: classe,
        type: "button",
        dataset: dados,
        onclick: () => {
          fechar();
          fazer();
        },
      }, texto));
    fechar = abrirFolha(
      folha(id, titulo, ...antes, h("div", { class: "cel-folha-acoes" }, botoes)),
      () => fechar(),
    );
    botoes[0]?.focus();
  }

  // ------------------------------------------------------- SENHA DA SALA

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
      fechar = abrirFolha(
        folha("cel-folha-senha", PALAVRAS.senhaDaSala,
          h("p", { class: "cel-folha-nota", text: PALAVRAS.pedeSenha(sala) }),
          h("label", { class: "rotulo", for: "cel-senha-campo", text: PALAVRAS.senha }),
          campo,
          h("div", { class: "cel-folha-acoes" },
            h("button", { class: "botao", type: "button", "data-resposta": "entrar", onclick: () => responder(campo.value) },
              PALAVRAS.entrar),
            h("button", { class: "botao-fantasma", type: "button", "data-resposta": "agora-nao", onclick: () => responder(null) },
              PALAVRAS.agoraNao))),
        () => responder(null),
      );
      campo.focus();
    });
  }

  CASCA.pedirSenhaDaSala = pedirSenhaDaSala;

  // --------------------------------------------------- a folha da mensagem

  /**
   * As ações sobre uma mensagem, que no desktop são o `remover` ao lado da hora
   * e a seleção do texto com o mouse.
   *
   * `remover` aperta o `remover` do desktop, que pergunta antes; ele só existe
   * na própria mensagem. Na dos outros, remover é moderar, e moderar é pelo
   * SEELE no computador: a frase aparece só para quem tem a permissão.
   */
  function folhaDaMensagem({ autor, hora, texto, propria, podeRemoverDosOutros, remover }) {
    const acoes = [{ texto: PALAVRAS.copiar, dados: { acao: "copiar" }, fazer: () => navigator.clipboard?.writeText(texto) }];
    if (propria) acoes.push({ texto: PALAVRAS.apagarMinha, dados: { acao: "apagar" }, fazer: remover });
    const antes = [h("p", { class: "cel-folha-nota cel-folha-citacao", text: texto })];
    if (!propria && podeRemoverDosOutros) {
      antes.push(h("p", { class: "cel-fora" }, h("span", { class: "rotulo", text: PALAVRAS.remover }), PALAVRAS.foraRemover));
    }
    folhaDeAcoes("cel-folha-mensagem", PALAVRAS.mensagemDe(autor, hora), antes, acoes);
  }

  // ------------------------------------------------------------- ANEXAR

  /**
   * FOTO OU VÍDEO e ARQUIVO pedem o arquivo pelo caminho do desktop. Quem abre
   * o seletor do navegador, dentro do toque, é a ponte de arquivos da F4.
   */
  function folhaDeAnexar(escolher) {
    folhaDeAcoes("cel-folha-anexar", PALAVRAS.anexar, [], [
      { texto: PALAVRAS.midia, dados: { escolha: "midia" }, fazer: () => escolher("midia") },
      { texto: PALAVRAS.arquivo, dados: { escolha: "arquivo" }, fazer: () => escolher("arquivo") },
    ]);
  }

  // --------------------------------------------------- a folha de uma frase

  /** O que o celular não faz, dito onde o controle estaria. */
  function folhaDeFrase(id, titulo, rotulo, frase) {
    folhaDeAcoes(id, titulo, [h("p", { class: "cel-fora" }, h("span", { class: "rotulo", text: rotulo }), frase)],
      [{ texto: PALAVRAS.fechar, fazer: () => {} }]);
  }

  CELULAR.registrar("folhaDaMensagem", folhaDaMensagem);
  CELULAR.registrar("folhaDeAnexar", folhaDeAnexar);
  CELULAR.registrar("folhaDeFrase", folhaDeFrase);
})();
