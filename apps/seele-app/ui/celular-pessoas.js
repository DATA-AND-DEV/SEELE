// SEELE — a aba PESSOAS do celular (F8).
//
// Spec: docs/superpowers/specs/2026-10-09-0.16-celular-design.md, §2.2.
//
// A lista é a do desktop (`#lista-roster`, desenhada pelo tela-sessao.js e
// reconciliada pelo id a cada quadro: a linha de uma pessoa é o mesmo nó de um
// quadro ao seguinte, desde o lote AX). O toque numa linha abre a ficha da
// pessoa (0.19), a mesma do desktop, numa folha que sobe da borda.

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "celular") return;

  const lista = document.getElementById("lista-roster");

  // **O toque na linha abre a ficha da pessoa** (0.19, pedido do dono): a
  // mesma do desktop (`abrirFichaDaPessoa`, em `tela-sessao.js`), que no
  // celular sobe da borda — o perfil, o estado, o volume de quem está na sua
  // sala e o que os MODs acrescentam. O sinal não é mais desenhado por pessoa.
  // A captura vem antes de qualquer outro ouvinte da lista. Um botão ou um
  // link que um MOD desenhou na linha continua fazendo o que é dele.
  lista.addEventListener("click", (evento) => {
    const linha = evento.target.closest("li.pessoa");
    if (!linha) return;
    const daFicha = evento.target.closest("button[data-ficha-pessoa]");
    if (!daFicha && evento.target.closest("a, button, input, select, textarea")) return;
    evento.stopPropagation();
    evento.preventDefault();
    abrirFolhaDaLinha(linha);
  }, true);

  function abrirFolhaDaLinha(linha) {
    if (linha.dataset.pessoaId) abrirFichaDaPessoa(linha.dataset.pessoaId);
  }

  // ------------------------------------------------------------- o filtro
  //
  // A busca e os recortes só escondem linhas da lista: o desenho e a
  // reconciliação continuam do desktop. «Na sala» é quem está num grupo com
  // ocupação (n/m); «Fora», quem está fora de sala.
  const filtro = document.getElementById("pessoas-filtro");
  filtro.hidden = false;
  const busca = document.getElementById("pessoas-busca");
  const recortes = [...filtro.querySelectorAll("[data-recorte]")];
  let recorte = "todas";

  const naSala = (linha) => /\//.test(linha.closest(".roster-grupo")?.querySelector(".roster-sala-conta")?.textContent ?? "");
  const semAcento = (texto) => texto.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();

  function aplicarFiltro() {
    const termo = semAcento(busca.value.trim());
    const contas = { todas: 0, sala: 0, fora: 0 };
    for (const linha of lista.querySelectorAll("li.pessoa")) {
      const dentro = naSala(linha);
      contas.todas += 1;
      contas[dentro ? "sala" : "fora"] += 1;
      const nome = semAcento(linha.querySelector(".pessoa-nome")?.textContent ?? "");
      linha.hidden = (recorte === "sala" && !dentro) || (recorte === "fora" && dentro) || (termo !== "" && !nome.includes(termo));
    }
    for (const grupo of lista.querySelectorAll(":scope > .roster-grupo")) {
      const pessoas = grupo.querySelectorAll("li.pessoa");
      // O grupo de quem está fora do ar não tem pessoa: só some quando se filtra.
      grupo.hidden = pessoas.length
        ? ![...pessoas].some((linha) => !linha.hidden)
        : (recorte !== "todas" || termo !== "");
    }
    for (const [nome, valor] of Object.entries(contas)) {
      const conta = filtro.querySelector(`[data-conta="${nome}"]`);
      if (conta) conta.textContent = String(valor);
    }
  }

  for (const botao of recortes) {
    botao.addEventListener("click", () => {
      recorte = botao.dataset.recorte;
      for (const outro of recortes) outro.setAttribute("aria-pressed", String(outro === botao));
      aplicarFiltro();
    });
  }
  busca.addEventListener("input", aplicarFiltro);
  // Cada quadro do desktop pode trazer linhas novas: elas nascem sem o filtro.
  new MutationObserver(aplicarFiltro).observe(lista, { childList: true, subtree: true });
  aplicarFiltro();

  // Pelo apelido, para quem abre a folha de fora da lista (a grade da CHAMADA).
  // O alias interno resolve o PersonId, inclusive para quem não tem deslizante.
  CELULAR.registrar("abrirPessoa", (apelido) => {
    const linhas = [...lista.querySelectorAll("li.pessoa")];
    const pessoas = [...(desenhado?.presentes ?? []),
      ...(desenhado?.voice_rooms ?? []).flatMap(sala => sala.people ?? [])];
    const pessoa = pessoas.find(p => p.nickname === apelido);
    const linha = pessoa && linhas.find(l => l.dataset.pessoaId === String(pessoa.id));
    if (linha) abrirFolhaDaLinha(linha);
  });
})();
