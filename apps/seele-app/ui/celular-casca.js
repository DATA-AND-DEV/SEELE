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
  });
})();
