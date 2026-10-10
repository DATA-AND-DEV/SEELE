// SEELE — a moldura do celular: as quatro abas e a faixa de voz (F8).
//
// Spec: docs/superpowers/specs/2026-10-09-0.16-celular-design.md, §2.2, §2.3 e §9.
//
// A moldura é montada aqui, por `createElement`, e só com a marca do celular: no
// desktop este arquivo volta na primeira linha, e a página do desktop não ganha
// nó nenhum. Ela não fala com o Rust (G5): MUDO, SURDO e SAIR apertam os
// controles do operador do desktop, que continuam no DOM, escondidos, e o
// caminho até o servidor é o de sempre. Ela se redesenha pelo mesmo snapshot,
// pelo `CASCA.depoisDeDesenhar` (P5 e P6).

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "celular") return;

  const { h } = CELULAR;
  const sessao = document.getElementById("tela-sessao");
  const doDesktop = (id) => document.getElementById(id);

  // As frases são do `frases.js` (P10): o `tests/celular.rs` confere que este
  // arquivo não escreve nenhuma.
  const PALAVRAS = FRASES_DO_CELULAR.voz;

  // ------------------------------------------------------------- as abas

  const ABAS = ["servidor", "conversa", "pessoas", "ajustes"];
  const abas = h("nav", { class: "cel-abas", "aria-label": PALAVRAS.secoes },
    ABAS.map((aba) => h("button", {
      id: `cel-aba-${aba}`,
      class: "cel-aba",
      type: "button",
      dataset: { aba },
      // A aba AJUSTES é a saída do tema, e lê o núcleo (spec, §6).
      "data-nucleo": aba === "ajustes",
      onclick: () => abrirAba(aba),
    }, PALAVRAS.abas[aba])));

  function abrirAba(aba) {
    sessao.dataset.aba = aba;
    for (const botao of abas.children) {
      if (botao.dataset.aba === aba) botao.setAttribute("aria-current", "page");
      else botao.removeAttribute("aria-current");
    }
  }

  // ------------------------------------------------------- a faixa de voz

  const nome = h("span", { class: "voz-sala-nome" });
  const conta = h("span", { class: "voz-sala-conta" });
  const quem = h("span", { class: "voz-sala-quem" });
  // Um toque na faixa abre a CHAMADA, que mora na coluna da conversa; dentro
  // dela, a mesma faixa, no mesmo lugar, diz RECOLHER e a fecha.
  const dentroDaSala = h("span", { class: "voz-sala-cabeca" }, nome, conta);
  // Com a letra do nome da sala, para a faixa não mudar de altura ao trocar.
  const recolher = h("span", { class: "voz-sala-cabeca voz-sala-nome", hidden: true }, glifo("aberto"), ` ${PALAVRAS.recolher}`);
  const sala = h("button", {
    class: "voz-sala",
    type: "button",
    onclick: () => {
      if (sessao.hasAttribute("data-chamada")) {
        fecharChamada();
        return;
      }
      abrirAba("conversa");
      abrirChamada();
    },
  }, dentroDaSala, recolher, quem);

  const sair = h("button", {
    id: "voz-sair",
    class: "voz-nucleo",
    type: "button",
    "aria-label": PALAVRAS.sairDaSala,
    // O SAIR do operador também desliga de um servidor; o da faixa só sai da
    // sala, e por isso só aperta o de lá quando ele está apontado para a sala.
    onclick: () => {
      const doOperador = doDesktop("operador-sair");
      if (doOperador.dataset.alvo === "sala") doOperador.click();
    },
  }, glifo("sair"), PALAVRAS.sair);
  const surdo = h("button", {
    id: "voz-surdo",
    class: "voz-nucleo",
    type: "button",
    onclick: () => doDesktop("botao-surdo").click(),
  });
  const mudo = h("button", {
    id: "voz-mudo",
    class: "voz-nucleo",
    type: "button",
    // No SEGURAR o toque não alterna nada: quem fala é o dedo parado em cima.
    onclick: () => {
      if (!modoSegurar) doDesktop("botao-mudo").click();
    },
  });

  const faixa = h("section", { id: "voz", class: "voz-celular", hidden: true },
    sala, h("div", { class: "voz-controles" }, sair, surdo, mudo));

  sessao.append(faixa, abas);
  abrirAba("conversa");

  // ------------------------------------------------- SEGURAR PARA FALAR
  //
  // Com o `voice_mode` em PushToTalk, o MUDO vira SEGURE: fala só enquanto o
  // dedo está nele, pelo `segurarFala` do desktop, o mesmo da tecla (spec,
  // §2.3). Todo jeito de o dedo deixar de segurar fecha a fala: soltar, o
  // sistema cancelar o toque, o dedo sair do controle, a página sumir, sair da
  // sala e o enlace cair. Um microfone aberto depois do dedo é o pior defeito
  // que esta tela pode ter.

  let modoSegurar = false;
  let segurando = false;
  let mutado = false;

  function soltar() {
    if (!segurando) return;
    segurando = false;
    segurarFala(false);
    desenharMudo();
  }

  mudo.addEventListener("pointerdown", (evento) => {
    if (!modoSegurar) return;
    evento.preventDefault();
    // Um toque prende o ponteiro ao alvo, e o `pointerleave` só chegaria
    // depois de o dedo levantar. Solto, ele chega quando o dedo sai do
    // controle, e a fala fecha ali.
    try {
      if (mudo.hasPointerCapture(evento.pointerId)) mudo.releasePointerCapture(evento.pointerId);
    } catch {
      // Um ponteiro que não existe mais não tem o que soltar.
    }
    segurando = true;
    segurarFala(true);
    desenharMudo();
  });
  for (const fim of ["pointerup", "pointercancel", "pointerleave", "touchcancel"]) {
    mudo.addEventListener(fim, soltar);
  }
  // O menu do toque longo não abre sobre o SEGURE.
  mudo.addEventListener("contextmenu", (evento) => {
    if (modoSegurar) evento.preventDefault();
  });
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) soltar();
  });

  let mudoDesenhado = null;
  function desenharMudo() {
    const chave = modoSegurar ? `segurar:${segurando}` : `mudo:${mutado}`;
    if (chave === mudoDesenhado) return;
    mudoDesenhado = chave;
    if (modoSegurar) {
      mudo.dataset.modo = "segurar";
      mudo.setAttribute("aria-pressed", String(segurando));
      mudo.setAttribute("aria-label", PALAVRAS.segurarParaFalar);
      mudo.replaceChildren(glifoComEstado("microfone", !segurando), segurando ? PALAVRAS.noAr : PALAVRAS.segure);
    } else {
      delete mudo.dataset.modo;
      mudo.setAttribute("aria-pressed", String(mutado));
      mudo.setAttribute("aria-label", mutado ? PALAVRAS.micFechado : PALAVRAS.micAberto);
      mudo.replaceChildren(glifoComEstado("microfone", mutado), mutado ? PALAVRAS.mudo : PALAVRAS.mic);
    }
  }

  // Um controle só troca de glifo e de palavra quando o estado dele muda: o
  // snapshot chega a cada 500 ms, e redesenhar o mesmo SVG a cada vez é
  // trabalho e é ruído para o leitor de tela.
  const estadoDe = new Map();
  function desenharControle(botao, ligado, glifoDele, palavras, rotulos) {
    if (estadoDe.get(botao) === ligado) return;
    estadoDe.set(botao, ligado);
    botao.setAttribute("aria-pressed", String(ligado));
    botao.setAttribute("aria-label", ligado ? rotulos[1] : rotulos[0]);
    botao.replaceChildren(glifoComEstado(glifoDele, ligado), ligado ? palavras[1] : palavras[0]);
  }

  CASCA.depoisDeDesenhar.push((snapshot) => {
    const aqui = (snapshot.voice_rooms ?? []).find((s) => s.occupied_by_us);
    // Fora de sala, com o enlace caído ou fora do PushToTalk, ninguém segura.
    // O `link` é o `LinkState` de `seele-ffi`: "Online", ou a bateria interna
    // (`{ InternalBattery: {…} }`) quando cai — como a CHAMADA do desktop lê.
    const segurarAgora = snapshot.voice_mode === "PushToTalk";
    if (!aqui || snapshot.link !== "Online" || !segurarAgora) soltar();
    modoSegurar = segurarAgora;
    faixa.hidden = !aqui;
    if (!aqui) return;
    faixa.setAttribute("aria-label", PALAVRAS.salaDeVoz(aqui.name));
    nome.textContent = aqui.name;
    conta.textContent = `${aqui.people.length}/${aqui.limit}`;

    // Enquanto o áudio abre, ou quando ele não abriu, a faixa diz isso no lugar
    // de quem fala, com a palavra do rodapé do desktop (matriz, B-F08c).
    const abertura = ABERTURA_DO_AUDIO[snapshot.abertura_do_audio];
    const falando = aqui.people.find((p) => p.speaking && !p.is_self);
    const sinal = PALAVRAS.sinal(snapshot.telemetry?.signal ?? 0);
    quem.textContent = abertura?.rodape
      ?? (falando ? `${PALAVRAS.fala(falando.nickname)} · ${sinal}` : `${PALAVRAS.naSala(aqui.people.length)} · ${sinal}`);

    mutado = !!snapshot.muted;
    desenharMudo();
    desenharControle(surdo, !!snapshot.total_isolation, "fone", [PALAVRAS.som, PALAVRAS.surdo],
      [PALAVRAS.somLigado, PALAVRAS.somDesligado]);
  });

  // A CHAMADA mora na coluna da conversa. Quando o desktop a abre (ao entrar
  // numa sala, pela faixa, ao começar uma transmissão), o celular mostra a aba
  // em que ela está, venha a abertura de onde vier.
  // ------------------------------------------------------------ a CHAMADA
  //
  // Ela cobre a sessão entre o topo e a faixa. A faixa fica onde estava, com o
  // MUDO no mesmo lugar, e a fileira da telemetria entra no lugar das abas, da
  // mesma altura.

  const vistaDaChamada = doDesktop("vista-chamada");
  const vistaAberta = () => !vistaDaChamada.hidden;
  let nomeDaSala = "";
  function desenharChamada() {
    const aberta = vistaAberta();
    sessao.toggleAttribute("data-chamada", aberta);
    dentroDaSala.hidden = aberta;
    recolher.hidden = !aberta;
    sala.setAttribute("aria-label", aberta ? PALAVRAS.recolherChamada : PALAVRAS.abrirChamada(nomeDaSala));
  }
  // A CHAMADA aberta é uma entrada no voltar do aparelho: o voltar a recolhe.
  let sairDoVoltar = null;
  new MutationObserver(() => {
    if (vistaAberta()) {
      abrirAba("conversa");
      sairDoVoltar ??= CELULAR.ajudante("empilhar")(() => {
        sairDoVoltar = null;
        fecharChamada();
      });
    } else {
      sairDoVoltar?.();
      sairDoVoltar = null;
    }
    desenharChamada();
  }).observe(vistaDaChamada, { attributes: true, attributeFilter: ["hidden"] });

  // A telemetria lê o que o rodapé do desktop já escreveu, com as palavras do
  // celular: PERDA, e não DISTÚRBIO (o glossário canônico).
  const valores = PALAVRAS.medidas.map(([, id]) => [id, h("span", { class: "cel-telemetria-valor" })]);
  sessao.append(h("div", { class: "cel-telemetria", role: "group", "aria-label": PALAVRAS.telemetria },
    PALAVRAS.medidas.map(([rotulo], i) => h("div", { class: "cel-telemetria-celula" },
      h("span", { class: "rotulo", text: rotulo }), valores[i][1]))));
  CASCA.depoisDeDesenhar.push((snapshot) => {
    nomeDaSala = (snapshot.voice_rooms ?? []).find((s) => s.occupied_by_us)?.name ?? "";
    desenharChamada();
    for (const [id, valor] of valores) valor.textContent = doDesktop(id)?.textContent ?? "";
  });

  // Um toque num cartão da grade abre a folha da pessoa, como na aba PESSOAS:
  // o volume dela mora lá, e não nas dez celas do cartão do desktop.
  doDesktop("chamada-grade").addEventListener("click", (evento) => {
    if (evento.target.closest("button, a, input")) return;
    const cartao = evento.target.closest("li[data-pessoa]");
    if (cartao) CELULAR.ajudante("abrirPessoa")?.(cartao.getAttribute("data-pessoa"));
  });

  CELULAR.registrar("abrirAba", abrirAba);
})();
