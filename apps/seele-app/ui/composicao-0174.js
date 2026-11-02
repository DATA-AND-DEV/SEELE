// Reorganiza os próprios controles, preservando IDs, eventos e estado.
"use strict";
(() => {
  const boot = document.getElementById("tela-boot");
  // `CELULAR` e `CASCA_LARGA` só existem na casca que os declara.
  const celular = typeof CELULAR !== "undefined";
  const caixa = boot?.querySelector(".boot-caixa");
  const topo = boot?.querySelector(".boot-topo");
  const proposito = boot?.querySelector(".boot-proposito");
  if (boot && caixa && topo) {
    // O celular segue com a introdução e o contexto de sempre; no desktop e no
    // navegador cada painel traz o próprio título (a entrada do 0.19).
    if (celular) {
      const introducao = document.createElement("div"); introducao.className = "inicio-introducao";
      const titulo = document.createElement("h1"); titulo.textContent = "Conversas";
      introducao.append(titulo); caixa.prepend(introducao);
      const contexto = document.createElement("span"); contexto.className = "inicio-contexto"; contexto.textContent = "Início";
      topo.querySelector(".boot-marca")?.after(contexto);
    }
    proposito?.remove();
    boot.prepend(topo);
    const faixa = boot.querySelector(".boot-faixa-web");
    if (faixa) {
      // De onde a página veio: a origem que a serviu, e não um nome escrito à mão (D-web-2).
      faixa.querySelectorAll("[data-origem-da-pagina]").forEach(nome => { nome.textContent = location.host; });
      boot.prepend(faixa);
    }
  }
  const esquerda = boot?.querySelector(".boot-esq");
  const direita = boot?.querySelector(".boot-dir");
  const codigo = document.getElementById("boot-codigo");
  const amigos = document.getElementById("boot-amigos");
  if (esquerda && direita && codigo && amigos) {
    const hospedar = document.createElement("div"); hospedar.className = "inicio-hospedar";
    while (direita.firstChild) hospedar.append(direita.firstChild);
    if (celular) {
      direita.append(codigo, hospedar);
      esquerda.append(amigos);
    } else {
      direita.append(hospedar);
    }
    const conectar = document.createElement("section");
    conectar.className = "inicio-conectar";
    conectar.setAttribute("aria-label", "Conectar");
    const larga = typeof CASCA_LARGA !== "undefined";
    const acao = document.getElementById("botao-conectar")?.closest(".boot-saidas");
    const dica = esquerda.querySelector(".boot-dica");
    if (celular) {
      const titulo = document.createElement("h2"); titulo.textContent = "Conectar";
      conectar.append(titulo);
    }
    if (acao) {
      const rotulo = document.createElement("label");
      rotulo.className = "campo";
      rotulo.textContent = celular ? "Link, endereço ou código"
        : larga ? "O link que alguém te mandou" : "Outro endereço, ou um link seele://";
      const entrada = document.createElement("input");
      entrada.type = "text";
      entrada.placeholder = celular ? "Cole aqui para conectar"
        : larga ? `https://${location.host}/#…` : "seele://… ou 10.0.0.4:8383";
      entrada.autocomplete = "off"; entrada.spellcheck = false;
      rotulo.append(entrada);
      const grupo = document.createElement("div"); grupo.className = "inicio-conectar-linha";
      grupo.append(rotulo, acao); conectar.append(grupo);
      const seguir = async () => {
        const valor = entrada.value.trim();
        await abrirServidores();
        if (valor) {
          document.getElementById("servidores-endereco").value = valor;
          entrada.value = "";
          document.getElementById("servidores-forma").requestSubmit();
        }
      };
      const botao = acao.querySelector("#botao-conectar");
      botao.addEventListener("click", event => { event.stopImmediatePropagation(); seguir(); }, true);
      entrada.addEventListener("keydown", event => {
        if (event.key === "Enter" && !event.isComposing) { event.preventDefault(); seguir(); }
      });
    }
    if (dica) dica.remove();
    esquerda.prepend(conectar);
  }
  const sessao = document.getElementById("tela-sessao");
  const lista = document.getElementById("lista-mods-navegacao");
  const palcoDePaginas = document.getElementById("palco-de-paginas");
  // A entrada da página aberta fica marcada como o canal aberto: `aria-current`
  // é o estado, e o CSS o pinta igual ao do canal. Vale nas duas cascas.
  const marcarAPaginaAberta = (conversa) => {
    if (!lista) return;
    const ativa = palcoDePaginas ? !palcoDePaginas.hidden : false;
    if (conversa) {
      if (ativa) conversa.removeAttribute("aria-current");
      else conversa.setAttribute("aria-current", "page");
    }
    const pagina = palcoDePaginas?.querySelector('[data-mod]:not([hidden])');
    for (const entrada of lista.querySelectorAll(".entrada-de-mod")) {
      if (ativa && pagina?.dataset.mod === entrada.dataset.modDaAcao) entrada.setAttribute("aria-current", "page");
      else entrada.removeAttribute("aria-current");
    }
    // O canal aberto deixa de ser «onde se está» enquanto uma página o cobre.
    sessao?.classList.toggle("com-pagina-de-mod", ativa);
  };
  // **Sem barra superior (0.19), nas duas cascas.** As páginas dos MODs ficam
  // na coluna de canais, sob «DOS MODS», onde `index.html` as declara — no
  // celular, a coluna é a aba SERVIDOR (canvas `Cel-servidor`). A volta à
  // conversa é escolher um canal (`alternarCanal` fecha a página) ou o VOLTAR
  // da própria página, que é do produto.
  if (sessao && lista) {
    const observar = () => marcarAPaginaAberta(null);
    if (palcoDePaginas) new MutationObserver(observar).observe(palcoDePaginas, { attributes: true, attributeFilter: ["hidden"], childList: true, subtree: true });
    new MutationObserver(observar).observe(lista, { childList: true, subtree: true });
  }
  const codigoEstado = document.getElementById("boot-codigo-estado");
  if (codigo && codigoEstado && PONTE_NO_LUGAR_DO_TAURI) {
    const tentar = document.createElement("button"); tentar.type = "button"; tentar.className = "botao-fantasma";
    tentar.textContent = "Tentar receber novamente"; tentar.hidden = true;
    const atualizar = () => { tentar.hidden = estadoDoCodigoPessoal()?.estado !== "Falhou"; };
    tentar.addEventListener("click", async () => {
      tentar.disabled = true;
      try { await window.seelePessoalWeb?.receber(true); }
      catch { /* O serviço já anunciou o estado e a mensagem correspondente. */ }
      finally { tentar.disabled = false; atualizar(); }
    });
    codigo.append(tentar);
    new MutationObserver(atualizar).observe(codigoEstado, { childList: true, characterData: true, subtree: true });
    atualizar();
  }
  // A janela estreita do app oferece os mesmos comandos da coluna recolhida.
  // Cada ação e cada estado pertencem ao controle original, nunca a uma cópia
  // do estado da chamada. O navegador celular já tem sua faixa persistente.
  const chamada = document.getElementById("vista-chamada");
  if (chamada) {
    const acoes = document.createElement("div"); acoes.className = "chamada-acoes-estreitas";
    for (const [id, nome] of [["botao-mudo", "Microfone"], ["botao-surdo", "Som"], ["operador-sair", "Sair da chamada"]]) {
      const original = document.getElementById(id);
      if (!original) continue;
      const botao = document.createElement("button"); botao.type = "button"; botao.className = "botao-fantasma";
      botao.textContent = nome;
      botao.addEventListener("click", () => original.click());
      const atualizar = () => {
        botao.disabled = original.disabled;
        botao.setAttribute("aria-label", original.getAttribute("aria-label") || original.title || nome);
        const pressionado = original.getAttribute("aria-pressed");
        if (pressionado !== null) botao.setAttribute("aria-pressed", pressionado);
      };
      atualizar(); new MutationObserver(atualizar).observe(original, { attributes: true });
      acoes.append(botao);
    }
    chamada.querySelector(".vista-chamada-barra")?.after(acoes);
  }

  // ------------------------------------------------- a chamada do canvas 0.19
  //
  // `Chamada`: na vista da chamada a coluna de canais sai de cena (o CSS a
  // esconde enquanto `#vista-chamada` está à vista), e a chamada traz o que
  // a coluna trazia: um CHAT que devolve à conversa do canal aberto, e a barra
  // de baixo com microfone, som, COMPARTILHAR, SAIR DA SALA e SAIR DO SERVIDOR.
  //
  // **Nenhum comando novo.** Cada botão daqui aperta o controle original do
  // operador (ou chama a mesma função que ele chama), e copia dele o estado —
  // rótulo, `aria-pressed`, `hidden`, `disabled` —, como a fileira estreita de
  // cima já fazia. O furo do véu dos MODs conta esta barra (`foraDoModal`, em
  // `mods-superficies.js`): com a coluna escondida, é ela que leva o microfone
  // e a saída, e nenhum modal de MOD a cobre.
  if (chamada && !celular) {
    const barra = chamada.querySelector(".vista-chamada-barra");
    if (barra && typeof fecharChamada === "function") {
      const chat = document.createElement("button");
      chat.type = "button";
      chat.id = "chamada-chat";
      chat.className = "chamada-chat";
      chat.title = "voltar à conversa do canal aberto, sem sair da sala";
      if (typeof glifo === "function") chat.append(glifo("linhas"));
      chat.append("CHAT");
      chat.addEventListener("click", () => fecharChamada());
      barra.append(chat);
    }
    const controles = document.createElement("div");
    controles.id = "chamada-controles";
    controles.className = "chamada-controles";
    controles.setAttribute("role", "toolbar");
    controles.setAttribute("aria-label", "Controles da chamada");
    const espelhar = (botao, original, { icone = false, rotulo = null, mostrar = () => !original.hidden } = {}) => {
      const atualizar = () => {
        botao.hidden = !mostrar();
        botao.disabled = original.disabled;
        const desligado = original.getAttribute("aria-disabled");
        if (desligado !== null) botao.setAttribute("aria-disabled", desligado);
        const pressionado = original.getAttribute("aria-pressed");
        if (pressionado !== null) botao.setAttribute("aria-pressed", pressionado);
        if (original.title) botao.title = original.title;
        if (icone) {
          // O desenho do original (o glifo com ou sem o corte) e o nome dele.
          // Snapshots chegam durante pointerdown/up: preservar o alvo mantém o clique no WebKit.
          if (botao.childNodes.length !== original.childNodes.length ||
              [...botao.childNodes].some((no, i) => !no.isEqualNode(original.childNodes[i]))) {
            botao.replaceChildren(...[...original.childNodes].map((no) => no.cloneNode(true)));
          }
          botao.setAttribute("aria-label", original.getAttribute("aria-label") || original.title || "");
        } else if (rotulo) {
          const texto = rotulo();
          if (botao.lastChild?.nodeType === Node.TEXT_NODE) botao.lastChild.textContent = texto;
          else botao.append(texto);
        }
      };
      atualizar();
      new MutationObserver(atualizar).observe(original, { attributes: true, childList: true, subtree: true, characterData: true });
      return atualizar;
    };
    const comGlifo = (nome) => {
      const b = document.createElement("button");
      b.type = "button";
      if (nome && typeof glifo === "function") b.append(glifo(nome));
      return b;
    };
    const mudo = document.getElementById("botao-mudo");
    const surdo = document.getElementById("botao-surdo");
    const vista = document.getElementById("operador-vista");
    const sair = document.getElementById("operador-sair");
    if (mudo && surdo && vista && sair) {
      const bMudo = comGlifo(null); bMudo.className = "chamada-controle";
      bMudo.addEventListener("click", () => mudo.click());
      espelhar(bMudo, mudo, { icone: true });
      const bSurdo = comGlifo(null); bSurdo.className = "chamada-controle";
      bSurdo.addEventListener("click", () => surdo.click());
      espelhar(bSurdo, surdo, { icone: true });
      // **Todos redondos e só ícone** (0.19, pedido do dono): o mesmo desenho do
      // cartão do operador — a tela, a porta e o × —, e a palavra no nome
      // acessível e no `title`. O desenho, o nome, o `aria-disabled` e o porquê
      // do indisponível vêm do botão original (`espelhar` com `icone`).
      const bVista = comGlifo(null); bVista.className = "chamada-controle";
      bVista.addEventListener("click", () => vista.click());
      espelhar(bVista, vista, { icone: true });
      const troca = document.getElementById("operador-trocar-tela");
      const bTroca = comGlifo(null); bTroca.className = "chamada-controle";
      bTroca.addEventListener("click", () => troca.click());
      espelhar(bTroca, troca, { icone: true, mostrar: () => !troca.hidden });
      // SAIR DO SERVIDOR: a mesma função do SAIR do operador fora de sala.
      const bServidor = comGlifo("porta"); bServidor.className = "chamada-controle";
      bServidor.setAttribute("aria-label", "SAIR DO SERVIDOR");
      bServidor.title = "sair deste servidor e voltar à tela de entrada";
      bServidor.addEventListener("click", () => {
        if (typeof sairDoServidorParaAEntrada === "function") sairDoServidorParaAEntrada();
      });
      // SAIR DA SALA: o × do operador, com o rótulo e o alvo dele.
      const bSala = comGlifo(null); bSala.className = "chamada-controle chamada-controle-sair";
      bSala.addEventListener("click", () => sair.click());
      espelhar(bSala, sair, { icone: true, mostrar: () => sair.dataset.alvo === "sala" });
      controles.append(bMudo, bSurdo, bVista, bTroca, bServidor, bSala);
      chamada.append(controles);
    }

    // A lista de pessoas recolhe numa faixa de 72px (o alternador do canvas).
    const pessoas = document.querySelector("#tela-sessao .painel-pessoas");
    const cartela = pessoas?.querySelector(".pessoas-cartela");
    if (pessoas && cartela) {
      const alternar = document.createElement("button");
      alternar.type = "button";
      alternar.id = "pessoas-recolher";
      alternar.className = "pessoas-recolher";
      const desenhar = () => {
        const recolhida = sessao?.dataset.pessoasRecolhidas === "sim";
        alternar.setAttribute("aria-expanded", recolhida ? "false" : "true");
        alternar.setAttribute("aria-label", recolhida ? "Abrir a lista de pessoas" : "Recolher a lista de pessoas");
        alternar.title = recolhida ? "abrir a lista de pessoas" : "recolher a lista de pessoas";
        alternar.replaceChildren(typeof glifo === "function" ? glifo(recolhida ? "recuar" : "avancar") : "");
      };
      // Recolhida, a faixa mostra um quadrado por pessoa: as iniciais (ou o
      // retrato) e o nome no `title`, lidos da lista de verdade — o produto
      // desenha, e não o MOD que apresenta a linha. Quem fala ganha o anel.
      const roster = document.getElementById("lista-roster");
      const mini = document.createElement("ul");
      mini.className = "pessoas-mini";
      mini.setAttribute("aria-label", "Pessoas no servidor");
      const redesenharMini = () => {
        if (sessao?.dataset.pessoasRecolhidas !== "sim" || !roster) return;
        const linhas = [...roster.querySelectorAll("li.pessoa[data-pessoa-id]")];
        mini.replaceChildren(...linhas.map((linha) => {
          const item = document.createElement("li");
          const quadro = linha.querySelector("[data-pessoa-do-avatar]");
          const nome = (linha.querySelector(".pessoa-nome")?.textContent || "").trim();
          const falando = linha.classList.contains("falando");
          item.className = "pessoas-mini-item";
          item.dataset.fala = falando ? "sim" : "nao";
          item.title = falando ? `${nome} · falando` : nome;
          item.setAttribute("aria-label", item.title);
          const imagem = quadro?.style.backgroundImage;
          if (imagem && imagem !== "none") item.style.backgroundImage = imagem;
          else item.textContent = (quadro?.dataset.iniciaisDoAvatar || nome).slice(0, 1).toUpperCase();
          return item;
        }));
      };
      if (roster) new MutationObserver(redesenharMini).observe(roster, { childList: true, subtree: true, attributes: true, attributeFilter: ["class", "style"] });
      alternar.addEventListener("click", () => {
        if (!sessao) return;
        if (sessao.dataset.pessoasRecolhidas === "sim") delete sessao.dataset.pessoasRecolhidas;
        else sessao.dataset.pessoasRecolhidas = "sim";
        desenhar();
        redesenharMini();
      });
      desenhar();
      cartela.after(alternar);
      alternar.after(mini);
    }
  }
})();
