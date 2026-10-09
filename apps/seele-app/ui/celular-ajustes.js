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

  // Para onde vai quem quer o app do SEELE (decisão da orquestradora, 09/10).
  // Vai para o `frases.js` com o P10, ao lado das frases.
  const PAGINA_DO_APP = "https://seele.app.br/#baixar";

  // Vão para `FRASES_DO_CELULAR`, no `frases.js`, com o P10 (Tarefa 14).
  const PALAVRAS = Object.freeze({
    ajustes: "AJUSTES",
    voltarAosAjustes: "Voltar à lista de AJUSTES",
    seuPerfil: "SEU PERFIL",
    editarPerfil: "EDITAR PERFIL",
    audio: "ÁUDIO",
    esteServidor: "ESTE SERVIDOR",
    modsDesteServidor: "MODS DESTE SERVIDOR",
    conexao: "CONEXÃO",
    conexaoSub: "As medidas do enlace com este servidor, também fora de sala.",
    medidas: [["ATRASO", "tel-rtt"], ["JITTER", "tel-jit"], ["PERDA", "tel-loss"], ["CODEC", "tel-codec"],
      ["CAMINHO", "tel-caminho"], ["UPTIME", "tel-uptime"], ["SINAL DA SALA", "tel-sinal-valor"]],
    foraDoCelular: "O QUE O CELULAR NÃO FAZ",
    foraDoCelularSub: "O que fica no app do SEELE, e o que é pelo SEELE no computador.",
    baixar: "BAIXAR O APP DO SEELE",
    grupos: [
      ["SÓ NO APP DO SEELE", [
        ["HOSPEDAR UM SERVIDOR", "Pôr um servidor no ar, o link de convite, a porta e o firewall ficam no app do SEELE, instalado num computador.", true],
        ["A PORTARIA DO SEU SERVIDOR", "Aprovar quem bate, a senha e os convites ficam no app do SEELE que hospeda o servidor."],
        ["INSTALAR E LIGAR MODS", "Os MODs deste servidor rodam aqui. Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE."],
        ["PACOTES DE MOD", "O espaço que os pacotes ocupam, as revisões guardadas e o APAGAR ficam no app do SEELE."],
        ["LIGAR PELO CÓDIGO", "Ligar pelo código é pelo app do SEELE: o navegador não abre o caminho que a ligação usa."],
        ["VERSÕES LADO A LADO", "O navegador abre a página da versão do servidor que o link pede. Guardar e abrir outras versões é do app do SEELE."],
        ["A TELA ENTRE PESSOAS", "No navegador, a tela de quem transmite vem sempre pelo servidor, e este aparelho não a repassa a ninguém."],
      ]],
      ["NO COMPUTADOR", [
        ["MODERAR", "Expulsar, banir, mover pessoas e remover mensagens de outros é pelo SEELE no computador."],
        ["SALAS E CANAIS", "Criar, renomear e apagar salas de voz e canais é pelo SEELE no computador."],
        ["NOME E IMAGEM DO SERVIDOR", "Pelo SEELE no computador. A aparência do servidor se edita aqui, em ESTE SERVIDOR."],
        ["COMPARTILHAR A TELA", "O navegador do celular não grava a tela. Compartilhar é pelo SEELE no computador, e a tela de quem compartilhar aparece na chamada."],
        ["TESTAR O MICROFONE FORA DE SALA", "O microfone do celular só abre numa sala de voz. Testar e ouvir a própria voz fora de sala é pelo SEELE no computador."],
        ["ESCOLHER A SAÍDA DE SOM", "O navegador do celular não escolhe a saída: troque entre fone, Bluetooth e alto-falante pelo controle de som do celular."],
      ]],
    ],
    sobre: "SOBRE",
    sairDoServidor: "SAIR DO SERVIDOR",
    segurarParaFalar: "SEGURAR PARA FALAR",
    segurarNota: "fala só enquanto o dedo está no SEGURE",
  });

  // A camada mora entre o topo e o que estiver embaixo (os avisos, a faixa e as
  // abas): as medidas vêm da moldura, e acompanham quando ela muda.
  function encaixar() {
    const topo = sessao.querySelector(".cel-topo").getBoundingClientRect().bottom;
    const pe = [...sessao.querySelectorAll(":scope > #veredito, :scope > #aviso-fone, :scope > #aviso-audio, :scope > .voz-celular, :scope > .cel-abas")]
      .filter((e) => !e.hidden && getComputedStyle(e).display !== "none")
      .reduce((menor, e) => Math.min(menor, e.getBoundingClientRect().top), innerHeight);
    camada.style.setProperty("--cel-ajustes-topo", `${topo}px`);
    camada.style.setProperty("--cel-ajustes-pe", `${innerHeight - pe}px`);
  }
  const observador = new ResizeObserver(encaixar);
  for (const parte of sessao.querySelectorAll(":scope > .cel-topo, :scope > .voz-celular, :scope > .cel-abas")) {
    observador.observe(parte);
  }
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

  // Tocar de novo na aba AJUSTES volta à raiz. Na captura, o ouvinte vê a aba
  // de antes do toque; o da troca de aba, no mesmo botão, vem depois.
  document.getElementById("cel-aba-ajustes").addEventListener("click", () => {
    if (sessao.dataset.aba === "ajustes") camada.setAttribute("data-cel-pagina", "lista");
  }, true);

  // ---------------------------------------------------------- SEU PERFIL
  //
  // No alto, o apelido de quem usa o aparelho, e EDITAR PERFIL, que abre o
  // perfil do desktop. A capa animada (D-capa-1) e a chave curta entram quando
  // a F3 (`pessoa.capa`) e a F4 (`identidade-web.js`) as entregarem.

  const coluna = camada.querySelector(".server-coluna");
  const apelido = h("span", { class: "cel-perfil-apelido" });
  const iniciais = h("span", { class: "cel-perfil-retrato", "aria-hidden": "true" });
  coluna.prepend(h("div", { class: "cel-perfil" },
    h("span", { class: "rotulo", text: PALAVRAS.seuPerfil }),
    h("div", { class: "cel-perfil-placa" }, iniciais, apelido),
    h("button", { class: "botao-fantasma cel-perfil-editar", type: "button", onclick: () => abrirPerfil() },
      PALAVRAS.editarPerfil)));

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

  const valores = PALAVRAS.medidas.map(([rotulo, id]) => [id, rotulo, h("span", { class: "cel-medida-valor" })]);
  const conexao = secaoNova("cel-secao-conexao", PALAVRAS.conexao, PALAVRAS.conexaoSub, "linhas",
    h("dl", { class: "cel-medidas" }, valores.map(([, rotulo, valor]) =>
      h("div", { class: "cel-medida" }, h("dt", { class: "rotulo", text: rotulo }), h("dd", {}, valor)))));

  const fora = secaoNova("cel-secao-fora", PALAVRAS.foraDoCelular, PALAVRAS.foraDoCelularSub, "desligar",
    PALAVRAS.grupos.map(([grupo, itens]) => h("section", { class: "cel-fora-grupo" },
      h("h3", { class: "rotulo", text: grupo }),
      h("ul", {}, itens.map(([nome, frase, comLink]) => h("li", {},
        h("strong", { text: nome }),
        h("p", { text: frase }),
        comLink && h("a", { class: "cel-baixar", href: PAGINA_DO_APP, target: "_blank", rel: "noopener", text: PALAVRAS.baixar })))))));

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
    h("li", {}, // Ações, e não seções: o `abrirSecao` do desktop passa por toda
    // `.server-secao` escondendo o painel dela, e estas não têm painel.
    h("button", { class: "cel-ajustes-acao cel-sobre", type: "button", onclick: () => abrirAjuda() },
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
  const tecla = document.getElementById("server-modo-tecla");
  tecla.querySelector(".server-modo-nome").textContent = PALAVRAS.segurarParaFalar;
  tecla.querySelector(".server-modo-desc").textContent = PALAVRAS.segurarNota;

  CASCA.depoisDeDesenhar.push(() => {
    apelido.textContent = document.getElementById("operador-nome").textContent;
    iniciais.textContent = document.getElementById("operador-inicial").textContent;
    for (const [id, , valor] of valores) valor.textContent = document.getElementById(id)?.textContent ?? "";
  });
})();
