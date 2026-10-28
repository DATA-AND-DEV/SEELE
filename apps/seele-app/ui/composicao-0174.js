// Reorganiza os próprios controles, preservando IDs, eventos e estado.
"use strict";
(() => {
  const boot = document.getElementById("tela-boot");
  const caixa = boot?.querySelector(".boot-caixa");
  const topo = boot?.querySelector(".boot-topo");
  const proposito = boot?.querySelector(".boot-proposito");
  if (boot && caixa && topo) {
    const introducao = document.createElement("div"); introducao.className = "inicio-introducao";
    const titulo = document.createElement("h1"); titulo.textContent = "Conversas";
    introducao.append(titulo); proposito?.remove(); caixa.prepend(introducao);
    const contexto = document.createElement("span"); contexto.className = "inicio-contexto"; contexto.textContent = "Início";
    topo.querySelector(".boot-marca")?.after(contexto);
    boot.prepend(topo);
  }
  const esquerda = boot?.querySelector(".boot-esq");
  const direita = boot?.querySelector(".boot-dir");
  const codigo = document.getElementById("boot-codigo");
  const amigos = document.getElementById("boot-amigos");
  if (esquerda && direita && codigo && amigos) {
    const hospedar = document.createElement("div"); hospedar.className = "inicio-hospedar";
    while (direita.firstChild) hospedar.append(direita.firstChild);
    direita.append(codigo, hospedar);
    esquerda.append(amigos);
    const conectar = document.createElement("section");
    conectar.className = "inicio-conectar";
    conectar.setAttribute("aria-label", "Conectar");
    const titulo = document.createElement("h2"); titulo.textContent = "Conectar";
    const acao = document.getElementById("botao-conectar")?.closest(".boot-saidas");
    const dica = esquerda.querySelector(".boot-dica");
    conectar.append(titulo);
    if (acao) {
      const rotulo = document.createElement("label");
      rotulo.className = "campo"; rotulo.textContent = "Link, endereço ou código";
      const entrada = document.createElement("input");
      entrada.type = "text"; entrada.placeholder = "Cole aqui para conectar";
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
  if (sessao && lista) {
    const abas = document.createElement("nav");
    abas.className = "abas-superiores";
    abas.setAttribute("aria-label", "Conversa e MODs do servidor");
    abas.addEventListener("keydown", event => {
      if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
      const botoes = [...abas.querySelectorAll("button")].filter(el => !el.disabled && el.getClientRects().length);
      const atual = botoes.indexOf(document.activeElement);
      if (atual < 0 || !botoes.length) return;
      event.preventDefault();
      const indice = event.key === "Home" ? 0 : event.key === "End" ? botoes.length - 1
        : (atual + (event.key === "ArrowRight" ? 1 : -1) + botoes.length) % botoes.length;
      botoes[indice].focus();
    });
    const conversa = document.createElement("button");
    conversa.type = "button"; conversa.textContent = "Conversa";
    conversa.setAttribute("aria-current", "page");
    conversa.addEventListener("click", () => {
      // A saída pública de cada superfície devolve o palco à conversa.
      const palco = document.getElementById("palco-de-paginas");
      const fechar = palco?.querySelector(".superficie-de-mod-sair");
      if (fechar) fechar.click();
    });
    const marca = document.createElement("span"); marca.className = "sessao-assinatura"; marca.textContent = "SEELE";
    const contexto = document.createElement("span"); contexto.className = "sessao-contexto";
    const fonte = document.getElementById("barra-servidor-nome");
    const copiarContexto = () => { contexto.textContent = fonte?.textContent || "Conversa"; };
    copiarContexto();
    if (fonte) new MutationObserver(copiarContexto).observe(fonte, { childList: true, characterData: true, subtree: true });
    abas.append(marca, contexto, conversa, lista);
    const paineis = sessao.querySelector(".paineis");
    if (paineis) sessao.prepend(abas);
    const atualizar = () => {
      const ativa = !document.getElementById("palco-de-paginas")?.hidden;
      if (ativa) conversa.removeAttribute("aria-current");
      else conversa.setAttribute("aria-current", "page");
      const pagina = document.getElementById("palco-de-paginas")?.querySelector('[data-mod]:not([hidden])');
      for (const entrada of lista.querySelectorAll(".entrada-de-mod")) {
        if (ativa && pagina?.dataset.mod === entrada.dataset.modDaAcao) entrada.setAttribute("aria-current", "page");
        else entrada.removeAttribute("aria-current");
      }
    };
    const palco = document.getElementById("palco-de-paginas");
    if (palco) new MutationObserver(atualizar).observe(palco, { attributes: true, attributeFilter: ["hidden"], childList: true, subtree: true });
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
})();
