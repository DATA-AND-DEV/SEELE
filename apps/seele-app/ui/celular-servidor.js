// SEELE — o topo da sessão e a aba SERVIDOR do celular (F8).
//
// Spec: docs/superpowers/specs/2026-10-09-0.16-celular-design.md, §2.2.
//
// O topo diz em que servidor se está e o estado da conexão, em toda aba: no
// desktop isso morava na barra da janela, que o celular não tem. A aba SERVIDOR
// é a coluna de salas e canais do desktop, desenhada pelo tela-sessao.js de
// sempre e reorganizada em cartões; este arquivo só acrescenta o que o celular
// diz a mais (as marcas escritas, o ENTRAR MUDO, a frase do que fica no
// computador). O acréscimo de cada sala entra na linha que o desktop monta,
// pelo `CASCA.vestirASala`, antes de ele reconciliar a coluna pelo id: o ENTRAR
// e o ENTRAR MUDO são os mesmos nós de um quadro ao seguinte, e o foco fica
// neles (pendência 63(b)).

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "celular") return;

  const { h } = CELULAR;
  const sessao = document.getElementById("tela-sessao");
  const doDesktop = (id) => document.getElementById(id);

  // As frases são do `frases.js` (P10): o `tests/celular.rs` confere que este
  // arquivo não escreve nenhuma.
  const PALAVRAS = FRASES_DO_CELULAR.servidor;

  // -------------------------------------------------------------- o topo

  // O texto e o estado vêm do que o desktop já escreve na barra da janela
  // (`desenharTopo`), escondida no celular: uma fonte só para as três frases do
  // `Trust`, e o enum em `data-padrao` decide a cor.
  const nomeDoDesktop = doDesktop("barra-servidor-nome");
  const subDoDesktop = doDesktop("barra-servidor-sub");
  const selo = h("span", { id: "cel-selo-seguro", class: "cel-selo-seguro" });
  const titulo = h("h1", { class: "cel-topo-titulo" });
  const topo = h("header", { class: "cel-topo" }, h("p", { class: "cel-topo-sobre" }, selo), titulo);
  sessao.append(topo);

  function desenharTopo() {
    selo.textContent = subDoDesktop.textContent;
    selo.dataset.padrao = subDoDesktop.dataset.padrao ?? "";
    // O título é da aba. CONVERSA e PESSOAS ainda trazem o cabeçalho do
    // desktop, e um segundo título ali seria o mesmo dito duas vezes.
    const aba = sessao.dataset.aba;
    titulo.textContent = aba === "servidor" ? nomeDoDesktop.textContent : aba === "ajustes" ? PALAVRAS.ajustes : "";
    titulo.title = aba === "servidor" ? nomeDoDesktop.title : "";
  }
  new MutationObserver(desenharTopo).observe(sessao, { attributes: true, attributeFilter: ["data-aba"] });

  // ------------------------------------------------------------- a trilha

  // As redes deitadas no alto da aba, num trilho que rola de lado.
  sessao.querySelector(".paineis > .trilha").setAttribute("data-rola-de-lado", "");

  // ------------------------------------------------------------ as salas

  // O MUDO antes da entrada, e não ao mesmo tempo: o MUDO do desktop pergunta
  // o estado antes de mandar o `set_muted`, e apertar os dois em seguida podia
  // pôr a pessoa na sala com o microfone aberto. Espera-se o quadro dizer
  // «mudo»; sem isso em três segundos, ninguém entra.
  let mutado = false;
  let quandoMudo = null;
  async function entrarMudo(sala) {
    if (!mutado) {
      const mudou = new Promise((resolver) => {
        quandoMudo = resolver;
      });
      doDesktop("botao-mudo").click();
      const prazo = new Promise((resolver) => setTimeout(() => resolver(false), 3000));
      if ((await Promise.race([mudou.then(() => true), prazo])) === false) {
        quandoMudo = null;
        anunciar(PALAVRAS.mudoNaoRespondeu);
        return;
      }
    }
    // O ENTRAR da sala pelo id, na hora do clique: o nó atravessa os quadros (a
    // coluna é reconciliada), mas a sala pode ter saído da lista entre o toque e
    // a resposta do MUDO, e aí não há o que apertar.
    doDesktop("lista-voice_rooms").querySelector(`button[data-voice_room="${sala}"]`)?.click();
  }

  function marca(classe, ...filhos) {
    return h("span", { class: `cel-marca ${classe}` }, ...filhos);
  }

  /**
   * O que o celular acrescenta a uma sala, na linha que o desktop monta (o
   * `CASCA.vestirASala`). Sempre o mesmo feitio para o mesmo estado, e sem
   * mexer na chave da linha: a reconciliação casa os filhos pela etiqueta e
   * pela primeira classe, e é isso que mantém os nós.
   */
  function adaptarSala(item, sala) {
    // Renomear é pelo SEELE no computador: o nome é só texto.
    const nome = item.querySelector("button.voice_room-nome");
    if (nome) nome.replaceWith(h("span", { class: "voice_room-nome", text: nome.textContent }));

    const cabeca = item.querySelector(".canal-cabeca");
    if (sala.occupied_by_us) cabeca.append(marca("cel-marca-aqui", PALAVRAS.aqui));
    if (sala.password_required) cabeca.append(marca("cel-marca-senha", glifo("chave"), ` ${PALAVRAS.comSenha}`));
    // LOTADA é dito, e o ENTRAR continua: quem recusa é o servidor, como no
    // desktop, e a lista pode estar um quadro atrás.
    if (!sala.occupied_by_us && sala.people.length >= sala.limit) {
      cabeca.append(marca("cel-marca-cheia", PALAVRAS.lotada));
    }

    // ENTRAR MUDO ao lado do ENTRAR: fora de sala não há faixa de voz, e o
    // áudio só abre ao entrar (D-52). Sem ele, quem quer chegar calado entra
    // com o microfone aberto e corre para o MUDO.
    const entrar = item.querySelector("button[data-voice_room]");
    if (!sala.occupied_by_us && entrar) {
      const fileira = h("div", { class: "cel-sala-entrar" });
      entrar.replaceWith(fileira);
      // O ouvinte é o do quadro em que o botão nasceu, e ele atravessa os
      // outros: por isso só o id, que é a chave da linha. O rótulo, com o nome
      // de agora, a reconciliação atualiza.
      const id = sala.id;
      fileira.append(entrar, h("button", {
        class: "botao-fantasma cel-entrar-mudo",
        type: "button",
        "aria-label": PALAVRAS.entrarMudo(sala.name),
        onclick: () => entrarMudo(id),
      }, glifoComEstado("microfone", true), PALAVRAS.mudo));
    }
  }

  // ------------------------------------------- o que fica no computador

  const listaDeCanais = doDesktop("lista-linhas");
  const foraSalas = h("p", { class: "cel-fora", hidden: true },
    h("span", { class: "rotulo", text: PALAVRAS.salasECanais }), PALAVRAS.foraSalas);
  listaDeCanais.after(foraSalas);

  // Escolher um canal é pedir para lê-lo: a conversa dele está na outra aba.
  listaDeCanais.addEventListener("click", (evento) => {
    if (evento.target.closest("button[data-linha]")) CELULAR.ajudante("abrirAba")("conversa");
  });

  CASCA.vestirASala = (item, sala) => adaptarSala(item, sala);

  CASCA.depoisDeDesenhar.push((snapshot) => {
    desenharTopo();
    mutado = !!snapshot.muted;
    if (mutado && quandoMudo) {
      quandoMudo();
      quandoMudo = null;
    }
    foraSalas.hidden = snapshot.may_manage_voice_rooms !== true;
  });
})();
