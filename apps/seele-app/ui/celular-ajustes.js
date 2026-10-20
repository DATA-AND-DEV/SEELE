// SEELE — a aba AJUSTES do celular (F8).
//
// Spec: docs/superpowers/specs/2026-10-09-0.16-celular-design.md, §2.2.
//
// AJUSTES é a tela de CONFIGURAÇÕES do desktop (`#tela-server`, de
// tela-server.js), aberta pelo `abrirServer` de sempre e mostrada como páginas
// da aba: a lista de seções, ou uma seção com «‹ AJUSTES». Ela mora entre o
// topo e a faixa de voz, que continuam vivos: o MUDO e as outras abas ficam a um
// toque. Sair da aba fecha a tela pelo `fecharServer` de sempre.

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "celular") return;

  const { h } = CELULAR;
  const sessao = document.getElementById("tela-sessao");
  const camada = document.getElementById("tela-server");

  // As frases são do `frases.js` (P10): o `tests/celular.rs` confere que este
  // arquivo não escreve nenhuma.
  const PALAVRAS = FRASES_DO_CELULAR.ajustes;

  // A camada mora entre o topo e o que estiver embaixo (os avisos, a faixa e as
  // abas): as medidas vêm da moldura, e acompanham quando ela muda.
  function encaixar() {
    const topo = sessao.querySelector(".cel-topo").getBoundingClientRect().bottom;
    const pe = [...sessao.querySelectorAll(":scope > :is(#veredito, #aviso-fone, #aviso-audio, .cel-selo-da-previa, #cel-aviso-persistencia, .voz-celular, .cel-abas)")]
      .filter((e) => !e.hidden && getComputedStyle(e).display !== "none")
      .reduce((menor, e) => Math.min(menor, e.getBoundingClientRect().top), innerHeight);
    camada.style.setProperty("--cel-ajustes-topo", `${topo}px`);
    camada.style.setProperty("--cel-ajustes-pe", `${innerHeight - pe}px`);
  }
  const observador = new ResizeObserver(encaixar);
  // Tudo o que a medida lê, e os avisos do pé também: um que nasce com a camada
  // aberta a encolhe. Medido antes, sem eles: o veredito que chegava com AJUSTES
  // aberto ficava inteiro embaixo da camada, e o toque no dispensar dele caía num
  // botão de AJUSTES.
  function vigiar() {
    const partes = ":scope > :is(.cel-topo, #veredito, #aviso-fone, #aviso-audio, .cel-selo-da-previa, #cel-aviso-persistencia, .voz-celular, .cel-abas)";
    for (const parte of sessao.querySelectorAll(partes)) observador.observe(parte);
  }
  vigiar();
  // O selo da prévia é do `celular-folhas.js`, que carrega antes deste; o aviso
  // da resposta do `persist()`, do `celular-entrada.js`, que carrega depois.
  new MutationObserver(vigiar).observe(sessao, { childList: true });
  window.addEventListener("resize", encaixar);

  // Entrar na aba abre a tela, e sair dela a fecha.
  new MutationObserver(() => {
    const naAba = sessao.dataset.aba === "ajustes";
    if (naAba && camada.hidden) {
      camada.setAttribute("data-cel-pagina", "lista");
      encaixar();
      abrirServer("tela-sessao");
    } else if (!naAba && !camada.hidden) {
      fecharServer();
    }
  }).observe(sessao, { attributes: true, attributeFilter: ["data-aba"] });

  // A tela fechada por outro caminho (o Escape do desktop) devolve a pessoa à
  // conversa, e não a uma aba vazia.
  new MutationObserver(() => {
    if (camada.hidden && sessao.dataset.aba === "ajustes") CELULAR.ajudante("abrirAba")("conversa");
  }).observe(camada, { attributes: true, attributeFilter: ["hidden"] });

  // As páginas: tocar uma seção a mostra sozinha; «‹ AJUSTES» volta à lista.
  camada.querySelector(".server-faixa").prepend(h("button", {
    class: "cel-ajustes-voltar",
    type: "button",
    "aria-label": PALAVRAS.voltarAosAjustes,
    onclick: () => camada.setAttribute("data-cel-pagina", "lista"),
  }, glifo("recuar"), PALAVRAS.ajustes));
  camada.addEventListener("click", (evento) => {
    if (evento.target.closest(".server-coluna .server-secao")) camada.setAttribute("data-cel-pagina", "secao");
  });

  // Uma seção aberta é uma entrada no voltar do aparelho: o voltar volta à
  // lista. Voltar à lista por outro caminho (o «‹ AJUSTES», a aba, sair da aba)
  // tira a entrada.
  let sairDoVoltar = null;
  new MutationObserver(() => {
    const naSecao = !camada.hidden && camada.getAttribute("data-cel-pagina") === "secao";
    if (naSecao) {
      sairDoVoltar ??= CELULAR.ajudante("empilhar")(() => {
        sairDoVoltar = null;
        camada.setAttribute("data-cel-pagina", "lista");
      });
    } else {
      sairDoVoltar?.();
      sairDoVoltar = null;
    }
  }).observe(camada, { attributes: true, attributeFilter: ["data-cel-pagina", "hidden"] });

  // Tocar de novo na aba AJUSTES volta à raiz. Na captura, o ouvinte vê a aba
  // de antes do toque; o da troca de aba, no mesmo botão, vem depois.
  document.getElementById("cel-aba-ajustes").addEventListener("click", () => {
    if (sessao.dataset.aba === "ajustes") camada.setAttribute("data-cel-pagina", "lista");
  }, true);

  // ---------------------------------------------------------- SEU PERFIL
  //
  // No alto, o apelido de quem usa o aparelho, e EDITAR PERFIL, que abre o
  // perfil do desktop. A placa veste a capa de quem usa (`pessoa.capa`,
  // D-capa-1) como a barra do operador do desktop: o `vestirCapa` da F3, a
  // cada quadro. A chave curta entra com a tela ENTRAR EM (Tarefa 10).

  const coluna = camada.querySelector(".server-coluna");
  const apelido = h("span", { class: "cel-perfil-apelido" });
  const iniciais = h("span", { class: "cel-perfil-retrato", "aria-hidden": "true" });
  const placa = h("div", { class: "cel-perfil-placa" }, iniciais, apelido);
  const perfil = h("div", { class: "cel-perfil" },
    h("span", { class: "rotulo", text: PALAVRAS.seuPerfil }),
    placa,
    h("button", { class: "botao-fantasma cel-perfil-editar", type: "button", onclick: () => abrirPerfil() },
      PALAVRAS.editarPerfil));
  coluna.prepend(perfil);

  // A região `operador.barra` (F3, passo 8b): a tira do operador não existe no
  // celular, e a região vem para logo abaixo de SEU PERFIL. O mesmo nó, e não
  // uma cópia: o `desenharOperador` a acha pelo id a cada quadro. O teto de
  // altura é da casca (`celular-sessao.css`): o que passar rola por dentro, e
  // a gestão de MODs, mais abaixo, continua ao alcance.
  perfil.after(document.getElementById("operador-mods"));

  // ----------------------------------------------------- as seções novas
  //
  // CONEXÃO e O QUE O CELULAR NÃO FAZ entram como seções de verdade: um botão
  // `.server-secao` com o painel dele, que o `abrirSecao` do desktop mostra e
  // esconde junto com as outras.

  const painelDoDesktop = camada.querySelector(".server-painel");
  function secaoNova(id, titulo, sub, glifoDela, ...conteudo) {
    const idDoPainel = id.replace("cel-secao-", "cel-painel-");
    painelDoDesktop.append(h("section", { id: idDoPainel, class: "server-painel-secao", hidden: true }, ...conteudo));
    const botao = h("button", {
      id,
      class: "server-secao",
      type: "button",
      "data-painel": idDoPainel,
      "data-titulo": titulo,
      "data-sub": sub,
      onclick: () => abrirSecao(id),
    }, h("span", { class: "server-secao-icone" }, glifo(glifoDela)),
      h("span", { class: "server-secao-texto" }, h("span", { class: "server-secao-nome", text: titulo })));
    return h("li", {}, botao);
  }

  const valores = Object.entries(PALAVRAS.medidas).map(([id, rotulo]) => [id, rotulo, h("span", { class: "cel-medida-valor" })]);
  const conexao = secaoNova("cel-secao-conexao", PALAVRAS.conexao, PALAVRAS.conexaoSub, "linhas",
    h("dl", { class: "cel-medidas" }, valores.map(([, rotulo, valor]) =>
      h("div", { class: "cel-medida" }, h("dt", { class: "rotulo", text: rotulo }), h("dd", {}, valor)))));

  const fora = secaoNova("cel-secao-fora", PALAVRAS.foraDoCelular, PALAVRAS.foraDoCelularSub, "desligar",
    PALAVRAS.grupos.map(([grupo, itens]) => h("section", { class: "cel-fora-grupo" },
      h("h3", { class: "rotulo", text: grupo }),
      h("ul", {}, Object.values(itens).map(([nome, frase, comLink]) => h("li", {},
        h("strong", { text: nome }),
        h("p", { text: frase }),
        comLink && h("a", { class: "cel-baixar", href: PAGINA_DO_APP, target: "_blank", rel: "noopener", text: PALAVRAS.baixar })))))));

  // ---------------------------------------------- como esta página abre
  //
  // A escolha da casca à mão (D-web-3): ela fica guardada neste navegador, e a
  // ponte a lê na próxima carga, antes do critério do aparelho. Nada troca no
  // meio da sessão: trocar de casca é abrir a página de novo.

  const estadoDaEscolha = h("p", { class: "casca-estado", role: "status" });
  const pelaTela = h("button", { class: "botao-fantasma", type: "button", "data-casca-escolhida": "auto", hidden: true,
    onclick: () => escolherCasca(null) }, PALAVRAS.pelaTela);
  function guardada() {
    try {
      return localStorage.getItem("seele.casca");
    } catch {
      return null;
    }
  }
  function escolherCasca(casca) {
    try {
      if (casca === null) localStorage.removeItem("seele.casca");
      else localStorage.setItem("seele.casca", casca);
      estadoDaEscolha.textContent = PALAVRAS.valeNaProxima;
    } catch {
      estadoDaEscolha.textContent = PALAVRAS.naoGuardou;
    }
    pelaTela.hidden = guardada() === null;
  }
  pelaTela.hidden = guardada() === null;
  const comoAbre = secaoNova("cel-secao-casca", PALAVRAS.comoAbre, PALAVRAS.comoAbreSub, "atualizar",
    h("p", { class: "cel-folha-nota", text: PALAVRAS.abriuComoCelular }),
    h("div", { class: "cel-folha-acoes" },
      h("button", { class: "botao", type: "button", "data-casca-escolhida": "larga", onclick: () => escolherCasca("larga") },
        PALAVRAS.abrirComoComputador),
      pelaTela),
    estadoDaEscolha);

  // --------------------------------------------------- a lista do celular
  //
  // As seções do desktop que o celular tem, na ordem do spec, com os nomes do
  // celular. As que são só do app (atalhos de teclado, a porta de quem hospeda,
  // versões, o caminho entre pares, a atualização) ficam nos grupos do
  // desktop, que o celular não mostra.

  function renomear(id, titulo) {
    const botao = document.getElementById(id);
    botao.setAttribute("data-titulo", titulo);
    botao.querySelector(".server-secao-nome").textContent = titulo;
  }
  renomear("secao-audio", PALAVRAS.audio);
  renomear("secao-servidor", PALAVRAS.esteServidor);
  renomear("secao-mods", PALAVRAS.modsDesteServidor);

  const doDesktop = (id) => document.getElementById(id).closest("li");
  coluna.append(h("ul", { class: "cel-ajustes-lista" },
    doDesktop("secao-audio"),
    doDesktop("secao-aparencia"),
    doDesktop("secao-servidor"),
    doDesktop("secao-mods"),
    conexao,
    doDesktop("secao-identidade"),
    fora,
    comoAbre,
    // Ações, e não seções: o `abrirSecao` do desktop passa por toda
    // `.server-secao` escondendo o painel dela, e estas não têm painel.
    h("li", {}, h("button", { class: "cel-ajustes-acao cel-sobre", type: "button", onclick: () => abrirAjuda() },
      h("span", { class: "server-secao-icone" }, glifo("comando")),
      h("span", { class: "server-secao-texto" }, h("span", { class: "server-secao-nome", text: PALAVRAS.sobre })))),
    h("li", {}, h("button", {
      class: "cel-ajustes-acao cel-sair-do-servidor",
      type: "button",
      // O mesmo caminho do SAIR DO SERVIDOR do operador de quem não hospeda, e
      // o celular nunca hospeda.
      onclick: () => sairDoServidorParaAEntrada(),
    }, h("span", { class: "server-secao-icone" }, glifo("sair")),
      h("span", { class: "server-secao-texto" }, h("span", { class: "server-secao-nome", text: PALAVRAS.sairDoServidor }))))));

  // O modo de voz do celular: SEGURAR PARA FALAR ou VOZ. O botão TECLA do
  // desktop já manda o PushToTalk, que no celular é o SEGURE da faixa; o ABERTO
  // não aparece (spec, §2.3).
  // A faixa de contexto diz o botão que se segura, e não a tecla: o celular não
  // tem ESPAÇO. Os outros modos dizem o que o desktop diz.
  CASCA.fraseDoModo = (modo, doDesktop) => (modo === "PushToTalk" ? PALAVRAS.modoSegurar : doDesktop);

  const tecla = document.getElementById("server-modo-tecla");
  tecla.querySelector(".server-modo-nome").textContent = PALAVRAS.segurarParaFalar;
  tecla.querySelector(".server-modo-desc").textContent = PALAVRAS.segurarNota;

  CASCA.depoisDeDesenhar.push((snapshot) => {
    apelido.textContent = document.getElementById("operador-nome").textContent;
    iniciais.textContent = document.getElementById("operador-inicial").textContent;
    vestirCapa(placa, snapshot.me);
    for (const [id, , valor] of valores) valor.textContent = document.getElementById(id)?.textContent ?? "";
  });
})();
