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
  // A aba mantém voz e navegação acessíveis fora da camada.
  camada.setAttribute("aria-modal", "false");

  // As frases são do `frases.js` (P10): o `tests/celular.rs` confere que este
  // arquivo não escreve nenhuma.
  const PALAVRAS = FRASES_DO_CELULAR.ajustes;

  // A camada mora entre o topo e o que estiver embaixo (os avisos, a faixa e as
  // abas): as medidas vêm da moldura, e acompanham quando ela muda.
  function encaixar() {
    const topo = sessao.querySelector(".cel-topo").getBoundingClientRect().bottom;
    const pe = [...sessao.querySelectorAll(":scope > :is(#veredito, #aviso-fone, #aviso-audio, .cel-selo-da-previa, #cel-aviso-persistencia, #cel-avisos-faixa, .voz-celular, .cel-abas)")]
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
    const partes = ":scope > :is(.cel-topo, #veredito, #aviso-fone, #aviso-audio, .cel-selo-da-previa, #cel-aviso-persistencia, #cel-avisos-faixa, .voz-celular, .cel-abas)";
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
    // CARGOS E PERMISSÕES (G): a camada sobre a sala, como no desktop. A aba
    // AJUSTES fecha antes, para a camada ficar sobre a conversa de que fala.
    typeof abrirCargos === "function" && h("li", {}, h("button", {
      class: "cel-ajustes-acao cel-cargos",
      type: "button",
      onclick: () => {
        fecharServer();
        abrirCargos({});
      },
    }, h("span", { class: "server-secao-icone" }, glifo("chave")),
      h("span", { class: "server-secao-texto" }, h("span", { class: "server-secao-nome", text: PALAVRAS.cargos })))),
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

// ------------------------------------------------- a faixa de avisos do pé

/**
 * Os avisos do pé da sessão no celular (o primeiro contato, o fone, o áudio, a identidade) eram quatro faixas fixas, cada uma com o texto inteiro
 * e o `dispensar`: três juntas tomavam ~40% da altura e a impressão digital inteira ia ao painel principal. Agora:
 *
 *   - quem protege (o veredito de identidade, e a resposta de persistência que alerta) fica SEMPRE à vista, em resumo de uma linha;
 *   - os que só informam (o fone, o áudio, a persistência «concedida») são recolhidos numa faixa «N AVISOS» com ABRIR/FECHAR;
 *   - aberta, a pilha tem altura máxima e rola dentro dela, e a faixa de voz abaixo nunca é coberta;
 *   - o texto continua o mesmo (de `frases.js`) e o `dispensar` de cada um continua onde o produto o dá; só a apresentação muda.
 */
(function faixaDeAvisos() {
  const sessao = document.getElementById("tela-sessao");
  if (!sessao || typeof FRASES_AVISOS_DO_CELULAR === "undefined") return;
  const f = FRASES_AVISOS_DO_CELULAR;
  const ids = ["veredito", "aviso-fone", "aviso-audio", "cel-aviso-persistencia"];
  const faixa = document.createElement("button");
  faixa.type = "button";
  faixa.id = "cel-avisos-faixa";
  faixa.className = "cel-avisos-faixa botao-fantasma";
  faixa.hidden = true;
  faixa.setAttribute("aria-controls", ids.join(" "));
  faixa.setAttribute("aria-expanded", "false");
  sessao.append(faixa);

  const ativo = (e) => e && !e.hidden && e.textContent.trim() !== "";
  const principal = (e) => e.id === "veredito" || (e.id === "cel-aviso-persistencia" && e.dataset.resposta && e.dataset.resposta !== "concedida");

  // A IMPRESSÃO DIGITAL nunca inteira no painel principal: no aviso do veredito ela vira «começo…fim», e o botão «ver inteira» (no próprio aviso,
  // com aria-expanded e aria-controls) a mostra e a oculta. O texto do produto não muda; só a apresentação.
  //
  // **A verdade é o texto VIGENTE do aviso.** Nada é guardado fora dele: o valor que o botão revela é o 64-hexadecimal que o texto de agora
  // tinha quando foi abreviado (`texto.dataset.inteira`), e só vale enquanto a forma curta dele continua no texto. Se o texto trocou para
  // outra impressão, ela é reconciliada do zero (curta de novo, botão ligado ao valor novo); se trocou para um texto sem impressão, o estado
  // é descartado e o botão some. A forma curta NUNCA é apresentada como a impressão: o botão diz que ela está abreviada.
  //
  // Escreve só quando a forma muda: depois de abreviada, o texto não casa mais com os 64 hexadecimais, e o observador não tem o que reescrever.
  const HEX64 = /[0-9a-fA-F]{64}/;
  const curta = (h) => `${h.slice(0, 8)}…${h.slice(-8)}`;
  function descartarAImpressao(texto, botao) {
    if (texto && "inteira" in texto.dataset) delete texto.dataset.inteira;
    if ("impressao" in sessao.dataset) delete sessao.dataset.impressao;
    if (botao && !botao.hidden) botao.hidden = true;
  }
  function impressaoNoAviso() {
    const texto = document.getElementById("veredito-texto");
    const aviso = document.getElementById("veredito");
    if (!texto || !aviso) return;
    const botao = document.getElementById("veredito-ver-inteira");
    if (aviso.hidden) return descartarAImpressao(texto, botao);
    const vigente = texto.textContent;
    const achou = HEX64.exec(vigente);
    let valor = null;
    if (achou) {
      // O texto vigente traz a impressão inteira: é dele que vem o valor. Expandida só se a pessoa pediu para ESTA impressão.
      valor = achou[0];
      if (!(sessao.dataset.impressao === "inteira" && texto.dataset.inteira === valor)) {
        texto.dataset.inteira = valor;
        delete sessao.dataset.impressao;
        texto.textContent = vigente.replace(valor, curta(valor));
      }
    } else if (texto.dataset.inteira && vigente.includes(curta(texto.dataset.inteira))) {
      valor = texto.dataset.inteira; // ainda a mesma impressão, abreviada
    }
    if (!valor) return descartarAImpressao(texto, botao);
    let b = botao;
    if (!b) {
      b = document.createElement("button");
      b.type = "button";
      b.id = "veredito-ver-inteira";
      b.className = "botao-fantasma";
      b.setAttribute("aria-controls", "veredito-texto");
      b.addEventListener("click", () => {
        const v = texto.dataset.inteira;
        if (!v) return;
        const inteira = sessao.dataset.impressao === "inteira";
        // Troca só dentro do texto vigente: se ele não traz a forma esperada, não há o que mostrar e o estado é reconciliado.
        const de = inteira ? v : curta(v);
        const para = inteira ? curta(v) : v;
        if (!texto.textContent.includes(de)) return organizar(null);
        texto.textContent = texto.textContent.replace(de, para);
        sessao.dataset.impressao = inteira ? "curta" : "inteira";
        organizar(null);
      });
      aviso.insertBefore(b, document.getElementById("veredito-fechar"));
    }
    const inteira = sessao.dataset.impressao === "inteira";
    const rotulo = inteira ? f.ocultar : f.verInteira;
    if (b.textContent !== rotulo) b.textContent = rotulo;
    const dizer = inteira ? f.ocultarRotulo : f.verInteiraRotulo;
    if (b.getAttribute("aria-label") !== dizer) b.setAttribute("aria-label", dizer);
    if (b.getAttribute("aria-expanded") !== String(inteira)) b.setAttribute("aria-expanded", String(inteira));
    if (b.hidden) b.hidden = false;
  }

  // **Sem laço**: a faixa mora dentro da `#tela-sessao`, que este observador vigia. Escrever nela sem comparar (o `hidden`, o
  // texto) gerava um registro novo a cada escrita, e o `organizar` se chamava para sempre: a página nem chegava ao
  // DOMContentLoaded (pwa-curto-11, tela branca). Os registros da PRÓPRIA faixa são ignorados, e cada escrita só acontece
  // quando o valor muda.
  function organizar(registros) {
    if (Array.isArray(registros) && registros.length && registros.every((r) => r.target === faixa || faixa.contains(r.target))) return;
    impressaoNoAviso();
    const vivos = ids.map((id) => document.getElementById(id)).filter(ativo);
    const aberto = sessao.dataset.avisos === "aberto";
    for (const e of ids.map((id) => document.getElementById(id))) {
      if (e) e.classList.toggle("cel-aviso-recolhido", vivos.includes(e) && !principal(e) && !aberto);
    }
    // A faixa existe enquanto houver aviso: fechada, cada um que protege é uma linha e os que informam somem atrás dela; aberta, todos inteiros.
    const esconder = vivos.length === 0;
    if (faixa.hidden !== esconder) faixa.hidden = esconder;
    if (!esconder) {
      const texto = `${f.faixa(vivos.length)} · ${aberto ? f.fechar : f.abrir}`;
      if (faixa.textContent !== texto) faixa.textContent = texto;
      if (faixa.getAttribute("aria-expanded") !== String(aberto)) faixa.setAttribute("aria-expanded", String(aberto));
    }
    if (esconder && "avisos" in sessao.dataset) delete sessao.dataset.avisos;
  }

  faixa.addEventListener("click", () => {
    sessao.dataset.avisos = sessao.dataset.avisos === "aberto" ? "fechado" : "aberto";
    organizar(null);
  });
  new MutationObserver(organizar).observe(sessao, { subtree: true, childList: true, attributes: true, attributeFilter: ["hidden", "data-resposta"], characterData: true });
  organizar(null);
})();
