// A prova da arquitetura híbrida, do lado do script.
//
// Três coisas, e nenhuma lógica nova:
// - a moldura (a faixa de voz e as abas) é montada por `createElement`, dentro
//   de `#tela-sessao`, só com a marca `data-casca="celular"`;
// - ela se redesenha pelo gancho depois de `desenhar(snapshot)` (o ponto de
//   extensão que a casca vai pedir à F5), com o mesmo `snapshot`;
// - as ações acionam o controle do desktop que já existe — o MUDO aperta o
//   `#botao-mudo` —, e o caminho até o servidor é o de sempre.

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "celular") return;

  const sessao = document.getElementById("tela-sessao");
  sessao.dataset.aba = "pessoas";

  const el = (tag, props = {}, ...filhos) => {
    const e = document.createElement(tag);
    for (const [k, v] of Object.entries(props)) {
      if (k === "text") e.textContent = v;
      else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
      else e.setAttribute(k, v);
    }
    e.append(...filhos);
    return e;
  };

  const nomeDaSala = el("span", { class: "voz-sala-nome" });
  const mudo = el("button", { id: "voz-mudo", class: "voz-nucleo", type: "button",
    onclick: () => document.getElementById("botao-mudo").click() });
  const surdo = el("button", { id: "voz-surdo", class: "voz-nucleo", type: "button",
    onclick: () => document.getElementById("botao-surdo").click() });
  const sair = el("button", { id: "voz-sair", class: "voz-nucleo", type: "button", text: "SAIR",
    onclick: () => {
      const botao = document.getElementById("operador-sair");
      if (botao.dataset.alvo === "sala") botao.click();
    } });
  const faixa = el("section", { id: "voz", class: "voz-celular" },
    el("div", { class: "voz-sala" }, nomeDaSala),
    el("div", { class: "voz-controles" }, sair, surdo, mudo));
  const abas = el("nav", { class: "cel-abas", "aria-label": "seções" },
    ...["servidor", "conversa", "pessoas", "ajustes"].map((a) => el("button", {
      class: "cel-aba", type: "button", text: a.toUpperCase(), "data-aba": a,
      onclick: () => { sessao.dataset.aba = a; desenharAbas(); },
    })));
  sessao.append(faixa, abas);

  function desenharAbas() {
    for (const b of abas.children) {
      if (b.dataset.aba === sessao.dataset.aba) b.setAttribute("aria-current", "page");
      else b.removeAttribute("aria-current");
    }
  }

  function desenharMoldura(snapshot) {
    const sala = (snapshot.voice_rooms ?? []).find((s) => s.occupied_by_us);
    faixa.hidden = !sala;
    if (!sala) return;
    nomeDaSala.textContent = `${sala.name} ${sala.people.length}/${sala.limit}`;
    mudo.setAttribute("aria-pressed", String(!!snapshot.muted));
    mudo.textContent = snapshot.muted ? "MUDO" : "MIC";
    surdo.setAttribute("aria-pressed", String(!!snapshot.total_isolation));
    surdo.textContent = snapshot.total_isolation ? "SURDO" : "SOM";
  }

  (globalThis.DEPOIS_DE_DESENHAR ||= []).push(desenharMoldura);
  desenharAbas();
})();
