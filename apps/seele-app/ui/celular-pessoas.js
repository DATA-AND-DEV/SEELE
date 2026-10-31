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

  // O sinal fica à vista na linha (specs/06: número e barra, nunca só cor). O
  // desktop recolhe o diagnóstico num `<details>`, e o celular o abre: o resumo
  // dele é escondido pela folha de estilo, e o toque é da linha.
  const abrirDiagnosticos = () => {
    for (const detalhe of lista.querySelectorAll("details.pessoa-diagnostico:not([open])")) detalhe.open = true;
  };
  new MutationObserver(abrirDiagnosticos).observe(lista, { childList: true, subtree: true });
  abrirDiagnosticos();

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
      // O id da pessoa, que a linha traz do mesmo `pessoa.id` da chave dela: é
      // o alvo das contribuições `pessoa.detalhes`, que a folha mostra.
      id: linha.dataset.pessoaId ?? null,
      nome: linha.querySelector(".pessoa-nome")?.textContent ?? "",
      iniciais: linha.querySelector(".pessoa-avatar")?.textContent ?? "",
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
      // VER PERFIL, só num servidor com perfis oficiais (0.19): num 0.18 com o
      // MOD PERFIS, o perfil é o que o MOD desenha, como sempre.
      verPerfil: linha.dataset.pessoaId && typeof perfisOficiaisDisponiveis === "function" && perfisOficiaisDisponiveis()
        ? () => abrirPerfilDaPessoa(linha.dataset.pessoaId)
        : null,
    });
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
