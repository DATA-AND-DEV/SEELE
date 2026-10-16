// SEELE — a aba PESSOAS do celular (F8).
//
// Spec: docs/superpowers/specs/2026-10-09-0.16-celular-design.md, §2.2.
//
// A lista é a do desktop (`#lista-roster`, desenhada pelo tela-sessao.js e
// reconciliada pelo id a cada quadro: a linha de uma pessoa é o mesmo nó de um
// quadro ao seguinte, desde o lote AX). O celular muda o toque: na linha de uma
// pessoa, ele abre a folha dela, com o sinal, o volume e onde se modera. No
// desktop, o nome abre a moderação, que no celular é pelo SEELE no computador.

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "celular") return;

  const lista = document.getElementById("lista-roster");

  // A captura vem antes do ouvinte de moderação do desktop, que mora na mesma
  // lista. Só o nome e as partes sem ação da linha abrem a folha: um botão ou
  // um link que um MOD desenhou na linha continua fazendo o que é dele.
  lista.addEventListener("click", (evento) => {
    const linha = evento.target.closest("li.pessoa");
    if (!linha) return;
    const doNome = evento.target.closest("button[data-moderar-pessoa]");
    if (!doNome && evento.target.closest("a, button, input, select, textarea")) return;
    evento.stopPropagation();
    evento.preventDefault();
    abrirFolhaDaLinha(linha);
  }, true);

  function abrirFolhaDaLinha(linha) {
    const deslizante = linha.querySelector("input.volume");
    const apelido = deslizante?.getAttribute("data-pessoa") ?? null;
    CELULAR.ajudante("folhaDaPessoa")({
      nome: linha.querySelector(".pessoa-nome")?.textContent ?? "",
      sinal: linha.querySelector(".pessoa-sync-valor")?.textContent ?? "",
      estado: linha.querySelector(".pastilha")?.textContent ?? "",
      volume: deslizante ? Number(deslizante.value) : null,
      // O deslizante da lista de agora, e não o do quadro em que a folha abriu.
      mudarVolume: (valor) => {
        const doDesktop = lista.querySelector(`input.volume[data-pessoa="${CSS.escape(apelido)}"]`);
        if (!doDesktop) return;
        doDesktop.value = String(valor);
        doDesktop.dispatchEvent(new Event("input", { bubbles: true }));
      },
      // O desktop só desenha a porta do nome para quem pode ser moderado por
      // quem usa o aparelho (`quemPodeSerModerado`): a regra é dele.
      podeModerar: !!linha.querySelector("button[data-moderar-pessoa]"),
    });
  }

  // Pelo apelido, para quem abre a folha de fora da lista (a grade da CHAMADA).
  // A linha de quem usa o aparelho não tem deslizante, e é achada pelo nome.
  CELULAR.registrar("abrirPessoa", (apelido) => {
    const linhas = [...lista.querySelectorAll("li.pessoa")];
    const linha = linhas.find((l) => l.querySelector("input.volume")?.getAttribute("data-pessoa") === apelido)
      ?? linhas.find((l) => !l.querySelector("input.volume") && l.querySelector(".pessoa-nome")?.textContent.startsWith(apelido));
    if (linha) abrirFolhaDaLinha(linha);
  });
})();
