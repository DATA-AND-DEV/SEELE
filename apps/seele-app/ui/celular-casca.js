// SEELE — a casca do celular: a raiz e os ajudantes que os outros celular-*.js usam.
//
// Spec: docs/superpowers/specs/2026-10-09-0.16-celular-design.md, §9.
//
// Tudo aqui mora numa IIFE cuja primeira linha volta sem a marca do celular: no
// desktop, este arquivo carrega e não faz nada (G4, em tests/celular.rs). Nenhum
// `invoke` (G5): a moldura do celular aciona os controles e as funções que o
// desktop já tem, e não abre outro caminho até o Rust. E nenhum `innerHTML`: todo
// nó nasce por `createElement`, e todo texto entra por `textContent`.

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "celular") return;

  /** Um elemento por `createElement`; texto sempre por `textContent`. */
  function h(tag, props = {}, ...filhos) {
    const el = document.createElement(tag);
    for (const [chave, valor] of Object.entries(props)) {
      if (valor === undefined || valor === null || valor === false) continue;
      if (chave === "class") el.className = valor;
      else if (chave === "text") el.textContent = valor;
      else if (chave.startsWith("on")) el.addEventListener(chave.slice(2), valor);
      else if (chave === "dataset") Object.assign(el.dataset, valor);
      else el.setAttribute(chave, valor === true ? "" : valor);
    }
    for (const filho of filhos.flat(Infinity)) {
      if (filho === null || filho === undefined || filho === false) continue;
      el.append(typeof filho === "string" ? document.createTextNode(filho) : filho);
    }
    return el;
  }

  // Os ajudantes que um celular-*.js oferece aos outros, por nome: a aba que se
  // abre, a folha que se mostra. Um mapa, e não globais soltos, porque o escopo
  // dos scripts da casca é um só e o desktop já mora nele.
  const ajudantes = new Map();
  globalThis.CELULAR = Object.freeze({
    h,
    registrar: (nome, fn) => ajudantes.set(nome, fn),
    ajudante: (nome) => ajudantes.get(nome),
    // O que um diálogo modal de MOD não deixa inerte (spec, §7): a faixa de voz
    // e as abas, com o MUDO, o SAIR e AJUSTES, que é o caminho para desligar um
    // MOD. O foco do teclado continua preso no diálogo. Quem pergunta é a pilha
    // de inércia do `mods-superficies.js` (F3), que não lê a marca.
    acordadosNoModal: () => [...document.querySelectorAll("#tela-sessao > .voz-celular, #tela-sessao > .cel-abas")],
  });

  const raiz = document.documentElement;
  const sessao = document.getElementById("tela-sessao");

  // ------------------------------------------------- o texto do sistema
  //
  // O tamanho de texto que a pessoa escolheu no aparelho, medido pela fonte do
  // sistema (`-apple-system-body`, que só o WebKit do iOS conhece), contra os
  // 17 px do corpo do iOS. O produto não encolhe abaixo do padrão nem passa do
  // dobro. Fora do iOS a sonda não existe, e a escala fica 1.

  function medirTextoDoSistema() {
    const sonda = h("span", { "aria-hidden": "true" }, "x");
    sonda.style.font = "-apple-system-body";
    if (!sonda.style.font) return 1;
    sonda.style.position = "absolute";
    sonda.style.visibility = "hidden";
    document.body.append(sonda);
    const tamanho = parseFloat(getComputedStyle(sonda).fontSize);
    sonda.remove();
    return Number.isFinite(tamanho) ? Math.min(Math.max(tamanho / 17, 1), 2) : 1;
  }
  function escrever() {
    const escala = medirTextoDoSistema();
    if (escala !== 1) raiz.style.setProperty("--cel-escala", String(escala));
  }
  escrever();
  // Quem troca o tamanho do texto nos ajustes do aparelho volta para a página.
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) escrever();
  });

  // ------------------------------------------------------------- o teclado
  //
  // O teclado do sistema não muda a página, só o pedaço dela que se vê
  // (`visualViewport`). Com ele aberto sobre um campo, a sessão ocupa só o que
  // se vê: o compositor fica colado a ele, as abas saem e a faixa de voz sobe
  // para o alto, com o MUDO à mão.

  const vista = window.visualViewport;
  function acompanharOTeclado() {
    const campo = document.activeElement;
    const escrevendo = !!campo && (campo.tagName === "TEXTAREA" || campo.tagName === "INPUT");
    // Contra a altura da raiz, e não a da janela: no Safari do iOS o
    // `innerHeight` encolhe junto com o teclado (medido no iOS 27: 714 sem ele,
    // 384 com ele, e o `clientHeight` da raiz em 714 nos dois).
    const aberto = escrevendo && vista.height < raiz.clientHeight - 120;
    sessao.toggleAttribute("data-teclado", aberto);
    if (aberto) {
      sessao.style.setProperty("--cel-visivel", `${vista.height}px`);
      sessao.style.setProperty("--cel-visivel-topo", `${vista.offsetTop}px`);
    }
  }
  if (vista) {
    vista.addEventListener("resize", acompanharOTeclado);
    vista.addEventListener("scroll", acompanharOTeclado);
    document.addEventListener("focusout", () => requestAnimationFrame(acompanharOTeclado));
  }

  // -------------------------------------------------------------- o voltar
  //
  // O voltar do aparelho (o botão do Android, o gesto do Safari) fecha a camada
  // de cima, e não a página: recarregar ou sair dela derruba a sessão.
  //
  // Uma entrada só no `history` enquanto houver camada aberta, e não uma por
  // camada. O `history.back()` é assíncrono: com uma entrada por camada, fechar
  // uma por outro caminho e abrir outra em seguida fazia o voltar atrasado tirar
  // a entrada da nova, e o fechamento dela saía da página (medido na bancada).
  //
  // - o voltar fecha a de cima, e rearma a entrada se ainda houver outra;
  // - fechar por outro caminho tira a camada da pilha; só a última devolve a
  //   entrada, com um voltar nosso, e uma camada que abrir antes de ele chegar
  //   espera por ele para rearmar.

  const pilha = [];
  let armado = false;
  let voltaNossa = false;
  function armar() {
    if (armado || voltaNossa || !pilha.length) return;
    history.pushState({ celular: true }, "");
    armado = true;
  }
  window.addEventListener("popstate", () => {
    armado = false;
    if (voltaNossa) voltaNossa = false;
    else pilha.pop()?.fechar();
    armar();
  });
  function empilhar(fecharPeloVoltar) {
    const entrada = { fechar: fecharPeloVoltar };
    pilha.push(entrada);
    armar();
    return () => {
      const onde = pilha.lastIndexOf(entrada);
      if (onde < 0) return;
      pilha.splice(onde, 1);
      if (!pilha.length && armado) {
        armado = false;
        voltaNossa = true;
        history.back();
      }
    };
  }
  CELULAR.registrar("empilhar", empilhar);
})();
