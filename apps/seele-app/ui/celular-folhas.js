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

  // As frases são do `frases.js` (P10): o `tests/celular.rs` confere que este
  // arquivo não escreve nenhuma.
  const PALAVRAS = FRASES_DO_CELULAR.folhas;

  let fecharAberta = null;
  // Uma entrada no voltar do aparelho para a folha aberta. Uma folha que toma o
  // lugar de outra herda a entrada dela, e não empilha outra.
  let sairDoVoltar = null;
  let desistirDaAberta = null;

  /**
   * Mostra uma folha e devolve a função que a fecha. Fechar devolve o foco a
   * quem o tinha antes, se esse nó ainda estiver na página.
   */
  function abrirFolha(folha, aoDesistir) {
    const herdada = fecharAberta !== null;
    if (herdada) fecharAberta(true);
    desistirDaAberta = aoDesistir;
    if (!herdada) sairDoVoltar = CELULAR.ajudante("empilhar")(() => {
      sairDoVoltar = null;
      desistirDaAberta?.();
    });
    const antes = document.activeElement;
    const veu = h("div", { class: "cel-veu", onclick: aoDesistir });
    // O Escape é da folha, e para nela: na captura do `window`, antes de todo
    // ouvinte do desktop. A CHAMADA fecha no Escape quando não há caixa por
    // cima, e sem isto a folha sumia primeiro e a CHAMADA fechava junto.
    const tecla = (evento) => {
      if (evento.key !== "Escape") return;
      evento.stopPropagation();
      evento.preventDefault();
      aoDesistir();
    };
    window.addEventListener("keydown", tecla, true);
    sessao.append(veu, folha);
    const fechar = (trocando = false) => {
      window.removeEventListener("keydown", tecla, true);
      veu.remove();
      folha.remove();
      fecharAberta = null;
      desistirDaAberta = null;
      if (trocando) return;
      sairDoVoltar?.();
      sairDoVoltar = null;
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

  // ----------------------------------------------------- a folha da pessoa

  /**
   * O sinal de uma pessoa, o volume dela e onde se modera.
   *
   * O volume sai da linha da lista e vem para cá: um deslizante por pessoa,
   * sempre à vista, faria da lista uma mesa de som, e no desktop ele só aparece
   * com o ponteiro em cima. A folha escreve no deslizante do desktop, que é quem
   * manda o `set_volume`. Quem usa o aparelho não tem volume para si.
   */
  // As contribuições `pessoa.detalhes` de quem a folha mostra (spec, § das
  // regiões: «o pessoa.detalhes sempre tem lugar na folha»; aprovado pela
  // orquestradora em 16/10). A contribuição é **um nó por pessoa** (o cache de
  // `montarContribuicao`, pelo id), e no celular só a folha o monta: a linha da
  // lista não (`CASCA.detalhesNaLinha`). A folha o põe ao abrir, o acompanha a
  // cada quadro enquanto aberta, sem trocar o nó que não mudou, e ele sai do
  // documento com ela, de volta ao cache.
  CASCA.detalhesNaLinha = false;
  let pintarOsDetalhes = null;
  CASCA.depoisDeDesenhar.push(() => pintarOsDetalhes?.());

  //
  // **De quem é cada seção** (núcleo de confiança; decidido pela orquestradora
  // em 16/10): acima do que o MOD desenha, a linha do produto com o nome dele
  // (o `data-mod` da raiz), nas cores do núcleo. É o par do «⋯» do desktop (F3,
  // `mods-controles.css`), com as mesmas classes: a linha
  // `contribuicao-de-mod-origem`, fora do nó da contribuição, e a caixa
  // `contribuicao-de-mod-caixa`, que recorta o que o MOD desenha: com `mover`,
  // `escalar` ou sombra, ele não cobre a linha. Sem ela, um «MODERAR» de MOD
  // entre o volume e o MODERAR do produto se passaria pelo produto.
  let secoesDeDetalhe = 0;

  function detalhesNaFolha(id) {
    const caixa = h("div", { class: "cel-folha-detalhes" });
    // Uma seção por nó, e a mesma de um quadro ao seguinte.
    const secoes = new Map();
    const secaoDe = (no) => {
      let secao = secoes.get(no);
      if (!secao) {
        secoesDeDetalhe += 1;
        const origem = `cel-detalhe-origem-${secoesDeDetalhe}`;
        secao = h("section", { class: "cel-folha-detalhe", "aria-labelledby": origem },
          h("p", { id: origem, class: "contribuicao-de-mod-origem", text: no.dataset.mod ?? "" }),
          h("div", { class: "contribuicao-de-mod-caixa" }, no));
        secoes.set(no, secao);
      }
      return secao;
    };
    const pintar = () => {
      if (!caixa.isConnected) {
        if (pintarOsDetalhes === pintar) pintarOsDetalhes = null;
        return;
      }
      const desejadas = conteudoDasContribuicoes("pessoa.detalhes", id).map(secaoDe);
      if (desejadas.length !== caixa.children.length || desejadas.some((secao, i) => caixa.children[i] !== secao)) {
        caixa.replaceChildren(...desejadas);
      }
    };
    return { caixa, pintar };
  }

  function folhaDaPessoa({ id, nome, sinal, estado, volume, mudarVolume, podeModerar }) {
    let fechar = null;
    let deslizante = null;
    const corpo = [h("div", { class: "cel-folha-sinal" },
      h("span", { class: "rotulo", text: PALAVRAS.sinal }),
      h("span", { class: "cel-folha-sinal-valor", text: sinal }),
      h("span", { class: "cel-folha-sinal-estado", text: estado }))];
    if (volume !== null) {
      const rotulo = h("span", { class: "rotulo cel-folha-volume", text: PALAVRAS.volume(volume) });
      deslizante = h("input", {
        type: "range",
        class: "cel-folha-deslizante",
        min: "0",
        max: "200",
        step: "10",
        value: String(volume),
        "aria-label": PALAVRAS.volumeDe(nome),
        oninput: () => {
          rotulo.textContent = PALAVRAS.volume(deslizante.value);
          mudarVolume(Number(deslizante.value));
        },
      });
      corpo.push(h("label", { class: "cel-folha-campo-volume" }, rotulo, deslizante));
    }
    const detalhes = id === null ? null : detalhesNaFolha(String(id));
    if (detalhes) corpo.push(detalhes.caixa);
    if (podeModerar) {
      corpo.push(h("p", { class: "cel-fora" }, h("span", { class: "rotulo", text: PALAVRAS.moderar }), PALAVRAS.foraModerar));
    }
    const fecharBotao = h("button", { class: "botao-fantasma", type: "button", onclick: () => fechar() }, PALAVRAS.fechar);
    corpo.push(h("div", { class: "cel-folha-acoes" }, fecharBotao));
    fechar = abrirFolha(folha("cel-folha-pessoa", nome, ...corpo), () => fechar());
    if (detalhes) {
      pintarOsDetalhes = detalhes.pintar;
      detalhes.pintar();
    }
    // O foco entra na folha, como nas outras: sem ele, o leitor da tela lia a
    // sessão por baixo dela (medido na árvore do WebKit).
    (deslizante ?? fecharBotao).focus();
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

  // ------------------------------------------------ o pé da moldura (§7)

  // As superfícies de MOD moram acima do pé da sessão: os avisos do produto, a
  // faixa de voz e as abas. `--cel-pe` é a distância do fim da sessão ao alto do
  // primeiro deles que aparece, e o `celular-folhas.css` posiciona os palcos por
  // ela. Medida, e não somada das alturas: a faixa só existe numa sala, os avisos
  // vão e vêm, e o teclado põe a faixa no alto.
  const PE = ":scope > :is(#veredito, #aviso-fone, #aviso-audio, .cel-selo-da-previa, #cel-aviso-persistencia, .voz-celular, .cel-abas, .cel-telemetria)";
  function medirOPe() {
    const fim = sessao.getBoundingClientRect().bottom;
    const altos = [...sessao.querySelectorAll(PE)]
      .filter((e) => !e.hidden && getComputedStyle(e).display !== "none")
      .map((e) => e.getBoundingClientRect().top);
    sessao.style.setProperty("--cel-pe", `${Math.max(0, fim - Math.min(fim, ...altos))}px`);
  }
  // Pela caixa de borda, e não pela de conteúdo, que é o padrão: as abas crescem
  // pela área segura de baixo como `padding` (com `box-sizing: border-box`), e a
  // caixa de conteúdo delas não muda. Medido na bancada: pela de conteúdo, o
  // observador às vezes não via a área segura chegar, e o diálogo terminava
  // 34 px abaixo do alto da faixa.
  const observador = new ResizeObserver(medirOPe);
  function vigiarOPe() {
    observador.observe(sessao, { box: "border-box" });
    for (const parte of sessao.querySelectorAll(PE)) observador.observe(parte, { box: "border-box" });
  }
  vigiarOPe();
  // O que entra no pé depois (o selo da prévia, abaixo; o aviso da resposta do
  // `persist()`, do `celular-entrada.js`, que carrega depois deste) entra na
  // medida também.
  new MutationObserver(vigiarOPe).observe(sessao, { childList: true });
  window.addEventListener("resize", medirOPe);
  medirOPe();

  CELULAR.registrar("folhaDaMensagem", folhaDaMensagem);

  // ------------------------------------------------------ a confirmação
  //
  // A caixa `#moderar` do desktop, que pergunta antes de todo ato sem volta
  // (apagar uma mensagem, o `confirmar` de um MOD), é no celular uma folha de
  // baixo (spec §2.4; o desenho é do `celular-folhas.css`). Como toda folha, o
  // voltar do aparelho a fecha, pelo mesmo caminho do Escape do desktop
  // (`fecharModeracao`). Sem isto, o voltar com a
  // confirmação aberta saía da página, e a sessão caía junto (medido na
  // bancada, caso `confirmacao`). A saída da folha é o CANCELAR dela, que o
  // desktop já põe embaixo do verbo; um FECHAR a mais seria o mesmo botão duas
  // vezes.
  const caixaDeConfirmar = document.getElementById("moderar");
  let tirarDaPilha = null;
  new MutationObserver(() => {
    if (!caixaDeConfirmar.hidden && !tirarDaPilha) {
      tirarDaPilha = CELULAR.ajudante("empilhar")(() => {
        tirarDaPilha = null;
        fecharModeracao();
      });
    } else if (caixaDeConfirmar.hidden && tirarDaPilha) {
      const tirar = tirarDaPilha;
      tirarDaPilha = null;
      tirar();
    }
  }).observe(caixaDeConfirmar, { attributes: true, attributeFilter: ["hidden"] });

  // ---------------------------------------------------- a prévia do tema
  //
  // D-estilo-1. No celular, o diálogo de um MOD cobre a sessão, e a prévia do
  // tema que ele pede não se vê atrás dele. O VER NA SESSÃO, no cabeçalho do
  // produto e só no diálogo que pediu a prévia, recolhe o diálogo num selo do
  // núcleo, acima da faixa, com VOLTAR; o voltar do aparelho também o devolve.
  // Recolher e devolver são do `mods-superficies.js` (`recolherSuperficie`):
  // o diálogo continua montado, com a prévia, e sai da pilha de inércia.
  //
  // O selo do desktop (`#selo-da-previa`, no alto e sem clique) o
  // `celular-folhas.css` esconde: este diz o mesmo, e é por onde se volta. Ele
  // aparece com qualquer prévia em vigor e nenhum diálogo de MOD à vista; o
  // VOLTAR, só quando há um diálogo que a casca recolheu.
  const palcoDeCamadas = document.getElementById("palco-de-camadas");
  const fraseDoSelo = h("span", { id: "cel-selo-da-previa-frase", text: PALAVRAS.seloDaPrevia });
  const voltarAoDialogo = h("button", {
    type: "button",
    "aria-describedby": "cel-selo-da-previa-frase",
    hidden: true,
    onclick: () => devolver(),
  }, PALAVRAS.voltarAoDialogo);
  const seloDaPrevia = h("div", {
    class: "cel-selo-da-previa",
    role: "status",
    "data-nucleo": true,
    hidden: true,
  }, fraseDoSelo, voltarAoDialogo);
  // Do pé: o `--cel-pe` o mede, e os palcos dos MODs ficam acima dele.
  sessao.append(seloDaPrevia);

  // O diálogo recolhido e a entrada dele no voltar. O foco de quem o abriu é do
  // `mods-superficies.js`, que o guarda no recolher e o repõe no devolver.
  let recolhida = null;

  function esquecerARecolhida() {
    const { tirarDoVoltar } = recolhida;
    recolhida = null;
    tirarDoVoltar();
  }

  function recolher(raiz) {
    if (recolhida || !recolherSuperficie(raiz)) return;
    recolhida = {
      raiz,
      // Tirar uma entrada que o voltar já tirou não faz nada.
      tirarDoVoltar: CELULAR.ajudante("empilhar")(() => devolver()),
    };
    pintarAPrevia();
    voltarAoDialogo.focus();
  }

  function devolver() {
    if (!recolhida) return;
    const { raiz } = recolhida;
    esquecerARecolhida();
    // Um diálogo que o MOD ocultou enquanto estava recolhido não volta: o
    // ocultar dele vale. Nada no palco muda, e quem tira o VOLTAR é este; o
    // foco, que estava nele, vai à sessão, e não ao `<body>`.
    if (!devolverSuperficie(raiz)) {
      pintarAPrevia();
      sessao.focus({ preventScroll: true });
    }
  }

  // Cada atribuição só quando muda: os observadores abaixo veem o que este
  // escreve, e escrever o mesmo valor de novo os chamaria sem fim.
  function mostrarSe(no, aVista) {
    if (no.hidden === aVista) no.hidden = !aVista;
  }

  function pintarAPrevia() {
    for (const raiz of palcoDeCamadas.querySelectorAll(".dialogo-de-mod")) {
      const cabeca = raiz.querySelector(":scope > .dialogo-de-mod-cabecalho");
      if (!cabeca) continue;
      let ver = cabeca.querySelector(":scope > .cel-ver-na-sessao");
      if (!ver) {
        ver = h("button", { type: "button", class: "cel-ver-na-sessao", hidden: true, onclick: () => recolher(raiz) },
          PALAVRAS.verNaSessao);
        const sair = cabeca.querySelector(":scope > .superficie-de-mod-sair");
        if (sair) sair.before(ver);
        else cabeca.append(ver);
      }
      mostrarSe(ver, previasDoTema.get(raiz.dataset.mod)?.superficie === raiz.dataset.superficie);
    }
    // Voltou por outro caminho (o MOD o mostrou) ou saiu (o MOD o fechou, ou a
    // sessão acabou): a entrada no voltar sai junto.
    if (recolhida && (!recolhida.raiz.isConnected || recolhida.raiz.checkVisibility())) esquecerARecolhida();
    const dialogoAVista = [...palcoDeCamadas.children].some((camada) => !camada.hidden);
    mostrarSe(seloDaPrevia, previasDoTema.size > 0 && !dialogoAVista);
    mostrarSe(voltarAoDialogo, recolhida !== null);
  }

  // A prévia muda pelo `escreverOTemaDaSessao`, que escreve o tema no estilo da
  // sessão e mostra ou esconde o selo do desktop; os diálogos, pelo palco.
  const vigia = new MutationObserver(pintarAPrevia);
  vigia.observe(palcoDeCamadas, { childList: true, subtree: true, attributes: true, attributeFilter: ["hidden"] });
  vigia.observe(sessao, { attributes: true, attributeFilter: ["style"] });
  const vigiarOSeloDoDesktop = () => {
    const doDesktop = document.getElementById("selo-da-previa");
    if (!doDesktop) return false;
    vigia.observe(doDesktop, { attributes: true, attributeFilter: ["hidden"] });
    return true;
  };
  if (!vigiarOSeloDoDesktop()) {
    const espera = new MutationObserver(() => {
      if (vigiarOSeloDoDesktop()) {
        espera.disconnect();
        pintarAPrevia();
      }
    });
    espera.observe(document.body, { childList: true });
  }
  pintarAPrevia();

  CELULAR.registrar("folhaDeAnexar", folhaDeAnexar);
  CELULAR.registrar("folhaDaPessoa", folhaDaPessoa);
  CELULAR.registrar("folhaDeFrase", folhaDeFrase);
})();
