// SEELE — a entrada do celular (F8, Tarefa 10).
//
// Spec: docs/superpowers/specs/2026-10-09-0.16-celular-design.md, §2.1.
//
// A tela de início, a «ENTRAR EM <endereço>?» e a de conectar moram numa seção
// própria dentro de `#tela-boot`, no lugar da caixa do desktop: quando a sessão
// abre, o desktop esconde `#tela-boot` como sempre, e esta seção vai junto.
//
// **O comportamento é da F4** (`identidade-web.js`, a 3.4): a tela lê o objeto
// de `ler()` e chama as ações dela. Ela não vê o token, não toca na chave e não
// conecta: o ENTRAR é o `entrar(apelido)` da identidade, e quem conecta é o
// `conectar` que a criou. A tela recebe a identidade pronta (`usar`), e nada
// aqui a cria.
//
// O aceite de MODs é o do desktop (`abrirAceiteDeMods`, de camada-mods.js), com
// a mesma lista e as mesmas frases; só o ACEITAR é desviado para o
// `aceitarOsMods` da identidade, que entra de novo com o mesmo token.

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "celular") return;

  const { h } = CELULAR;
  // As frases são do `frases.js` (P10): o `tests/celular.rs` confere que este
  // arquivo não escreve nenhuma.
  const PALAVRAS = FRASES_DO_CELULAR.entrada;
  const boot = document.getElementById("tela-boot");

  const corpo = h("div", { class: "cel-entrada-corpo" });
  const pe = h("div", { class: "cel-entrada-pe" });
  const tela = h("section", { class: "cel-entrada", "aria-labelledby": "cel-entrada-titulo" },
    // Decorativa, como a marca da entrada do desktop: o título diz que tela é esta.
    h("div", { class: "cel-entrada-marca" }, h("img", { src: "marca-assinatura.svg", alt: "" })),
    corpo,
    pe);
  boot.append(tela);

  /**
   * A identidade da página (F4), e de onde vêm os servidores deste aparelho: o
   * `conhecidos()` e o `entrarNoConhecido(impressao, apelido)` dela, ordenados
   * pelo `visto`, cada um com o `podeEntrar` (os hashes guardados vencem).
   */
  let identidade = null;
  let fontes = { conhecidos: async () => [], entrarNoConhecido: null };

  /** Troca o que a tela mostra; o título leva o foco, e o teclado não sobe sozinho. */
  function mostrar(conteudo, acoes) {
    corpo.replaceChildren(...conteudo.flat(Infinity).filter(Boolean));
    pe.replaceChildren(...acoes.filter(Boolean));
    pe.hidden = acoes.filter(Boolean).length === 0;
    corpo.scrollTop = 0;
    const titulo = corpo.querySelector("#cel-entrada-titulo");
    if (titulo && boot.contains(document.activeElement)) titulo.focus({ preventScroll: true });
  }

  const titulo = (...filhos) => h("h1", { id: "cel-entrada-titulo", class: "cel-entrada-titulo", tabindex: "-1" }, ...filhos);
  const rotulo = (texto) => h("span", { class: "rotulo", text: texto });
  const botao = (texto, onclick, classe = "botao") => h("button", { class: `${classe} cel-entrada-acao`, type: "button", onclick }, texto);

  /** Um aviso no corpo: o estado da tentativa, ou o desfecho do ENTRAR. */
  function aviso([cabeca, texto], { erro = false, extra = null } = {}) {
    return h("div", { class: "cel-entrada-aviso", role: erro ? "alert" : "status", "data-tipo": erro ? "erro" : null },
      h("p", { class: "cel-entrada-aviso-titulo", text: cabeca }),
      h("p", { class: "cel-entrada-nota", text: texto }),
      extra);
  }

  /** A data do link, em DD/MM, no relógio deste aparelho. */
  function dia(segundos) {
    const d = new Date(segundos * 1000);
    return `${String(d.getDate()).padStart(2, "0")}/${String(d.getMonth() + 1).padStart(2, "0")}`;
  }

  /**
   * A impressão inteira, em dezesseis grupos de quatro, quatro por fileira: é o
   * que se compara a olho com a que quem hospeda lê no SEELE dele. Quem escuta
   * a tela ouve os grupos em ordem, e não 64 letras corridas.
   */
  function impressaoEmGrupos(impressao) {
    const grupos = String(impressao).toUpperCase().match(/.{1,4}/g) ?? [];
    return h("span", { class: "cel-entrada-impressao", role: "text", "aria-label": grupos.join(" ") },
      grupos.map((g) => h("span", { "aria-hidden": "true", text: g })));
  }

  /** O apelido deste aparelho, o mesmo que a entrada do desktop mostra. */
  function apelidoDoAparelho() {
    const nome = document.getElementById("boot-perfil-nome")?.textContent.trim() ?? "";
    return nome === "—" ? "" : nome;
  }

  // O BAIXAR O APP DO SEELE é o mesmo da lista de O QUE O CELULAR NÃO FAZ.
  const PALAVRAS_DO_APP = FRASES_DO_CELULAR.ajustes.baixar;

  // -------------------------------------------------------- a tela de início

  /**
   * SERVIDORES DESTE APARELHO, cada um com ENTRAR (o ato explícito: daqui se
   * entra sem a «ENTRAR EM»), e COLAR UM LINK. O teclado só sobe com o toque
   * no COLAR.
   */
  async function inicio(desfecho = null) {
    const f = PALAVRAS.inicio;
    let lista = [];
    try {
      lista = await fontes.conhecidos();
    } catch (falha) {
      console.warn("celular-entrada, conhecidos:", falha);
    }
    const linhas = lista.length
      ? h("ul", { class: "cel-entrada-servidores" }, lista.map((s) => h("li", {},
        h("button", {
          class: "cel-entrada-servidor", type: "button",
          onclick: () => entrarNoConhecido(s),
          disabled: fontes.entrarNoConhecido ? null : true,
        },
        h("span", { class: "cel-entrada-servidor-texto" },
          h("span", { class: "cel-entrada-servidor-nome", text: s.nome || s.endereco }),
          h("span", { class: "cel-entrada-nota", text: [s.endereco, s.versao && f.servidor(s.versao), s.visto && dia(s.visto / 1000)].filter(Boolean).join(" · ") })),
        // Com os hashes guardados vencidos, o toque diz por que não entra.
        h("span", { class: "rotulo", text: s.podeEntrar === false ? f.linkVencido : f.entrar })))))
      : h("p", { class: "cel-entrada-nota", text: f.vazio });
    mostrar([
      titulo(f.titulo),
      desfecho?.erro && aviso(fraseDoDesfecho(desfecho), { erro: true }),
      linhas,
      h("p", { class: "cel-entrada-nota", text: f.identidade }),
      h("p", { class: "cel-entrada-fora" }, rotulo(f.hospedarRotulo), f.hospedar,
        h("a", { class: "cel-baixar", href: PAGINA_DO_APP, target: "_blank", rel: "noopener", text: PALAVRAS_DO_APP })),
    ], [botao(f.colar, colar)]);
  }

  async function entrarNoConhecido(servidor) {
    if (!fontes.entrarNoConhecido) return;
    await conectando(servidor.endereco, () => fontes.entrarNoConhecido(servidor.impressao, apelidoDoAparelho()), null, servidor);
  }

  // ------------------------------------------------------- COLAR UM LINK

  function colar(recusa = null) {
    const f = PALAVRAS.inicio;
    const campo = h("input", {
      type: "url", id: "cel-entrada-link", class: "cel-entrada-campo-texto",
      autocomplete: "off", autocapitalize: "none", spellcheck: "false", inputmode: "url",
    });
    const continuar = () => abrirOLink(campo.value.trim());
    campo.addEventListener("keydown", (evento) => {
      if (evento.key === "Enter") continuar();
    });
    mostrar([
      titulo(f.colar),
      h("label", { class: "cel-entrada-campo", for: "cel-entrada-link" }, rotulo(f.campoDoLink)),
      campo,
      recusa && aviso(recusa, { erro: true }),
    ], [botao(f.continuar, continuar), botao(PALAVRAS.entrarEm.agoraNao, () => inicio(), "botao-fantasma")]);
    // Este é o toque que pede o teclado.
    campo.focus();
  }

  /**
   * O que foi colado: um link do navegador vai à «ENTRAR EM»; um endereço
   * sozinho e um `seele://` não abrem conexão no navegador, e a tela diz por
   * quê antes de abrir a tentativa.
   */
  async function abrirOLink(texto) {
    if (!texto) return;
    if (!identidade) return colar(PALAVRAS.desfechos["sem-identidade"]);
    if (/^seele:/i.test(texto)) return colar(PALAVRAS.colado.doApp);
    if (!texto.includes("://") && !texto.includes("#")) return colar(PALAVRAS.colado.soEndereco);
    entrarEm(await identidade.abrir(texto));
  }

  // ---------------------------------------------------------- ENTRAR EM

  const BLOQUEIAM = ["vencido", "servidor-0.15", "incompleto", "outra-origem"];

  /**
   * A «ENTRAR EM <endereço>?». Sem nome de servidor: antes de conectar, o nome
   * seria só o que o link diz. Mostra o que se confere — o endereço, a
   * impressão inteira, a versão e até quando o link vale —, o apelido, e o
   * desfecho do último ENTRAR, quando houve um.
   */
  function entrarEm(lido, desfecho = null) {
    if (!lido) return inicio();
    const f = PALAVRAS.entrarEm;
    const estado = lido.estado;
    const doEstado = PALAVRAS.estados[estado];
    const bloqueia = BLOQUEIAM.includes(estado);
    // Um desfecho que encerrou a tentativa (o convite gasto ou vencido, a
    // recusa) não deixa ENTRAR de novo: o mesmo link não serve mais.
    const encerrou = desfecho && !["apelido-tomado", "sem-rede"].includes(desfecho.erro);

    const avisoDoEstado = doEstado && aviso(
      [typeof doEstado.titulo === "function" ? doEstado.titulo(dia(lido.valeAte)) : doEstado.titulo, doEstado.corpo],
      {
        erro: bloqueia,
        extra: doEstado.passos && h("ol", { class: "cel-entrada-passos" }, doEstado.passos.map((p) => h("li", { text: p }))),
      });

    const ficha = lido.endereco && h("dl", { class: "cel-entrada-ficha" },
      h("dt", {}, rotulo(f.endereco)), h("dd", { text: lido.endereco }),
      lido.impressao && [h("dt", {}, rotulo(f.impressao)), h("dd", {}, impressaoEmGrupos(lido.impressao))],
      lido.versaoDoServidor && [h("dt", {}, rotulo(f.versao)), h("dd", { text: PALAVRAS.inicio.servidor(lido.versaoDoServidor) })],
      lido.valeAte && [h("dt", {}, rotulo(f.vale)), h("dd", { text: dia(lido.valeAte) })]);

    const podeEntrar = !bloqueia && !encerrou && (estado === "pronto" || estado === "ja-conhecido");
    const campo = podeEntrar && h("input", {
      type: "text", id: "cel-entrada-apelido", class: "cel-entrada-campo-texto",
      autocomplete: "nickname", autocapitalize: "none", spellcheck: "false",
      "aria-invalid": desfecho?.erro === "apelido-tomado" ? "true" : null,
    });
    // O apelido tomado devolve o campo vazio e marcado; o resto, com o apelido
    // de antes, ou o deste aparelho.
    if (campo) campo.value = desfecho?.erro === "apelido-tomado" ? "" : (desfecho?.apelido ?? apelidoDoAparelho());

    const avisoDoDesfecho = desfecho && desfecho.erro && aviso(fraseDoDesfecho(desfecho), {
      erro: true,
      extra: desfecho.erro === "recusado" && desfecho.razao !== AINDA_NAO_NO_NAVEGADOR && desfecho.razao
        && h("p", { class: "cel-entrada-razao", text: MOTIVOS[desfecho.razao] ?? desfecho.razao }),
    });

    const acoes = [];
    if (estado === "numa-aba" && !encerrou) {
      const e = PALAVRAS.estados["numa-aba"];
      acoes.push(botao(e.copiar, async () => {
        let copiou = false;
        try {
          copiou = await identidade.copiarLink();
        } catch {
          copiou = false;
        }
        const nota = corpo.querySelector(".cel-entrada-copiado") ?? h("p", { class: "cel-entrada-copiado", role: "status" });
        nota.textContent = copiou ? e.copiado : e.naoCopiou;
        corpo.append(nota);
      }));
      if (lido.podeInstalar) acoes.push(botao(e.instalar, () => identidade.instalar(), "botao botao-secundario"));
      acoes.push(botao(f.agoraNao, agoraNao, "botao-fantasma"));
    } else if (podeEntrar) {
      acoes.push(botao(f.entrar, () => entrar(lido, campo.value.trim())));
      acoes.push(botao(f.agoraNao, agoraNao, "botao-fantasma"));
    } else {
      acoes.push(botao(f.servidores, agoraNao, "botao-fantasma"));
    }

    mostrar([
      titulo(f.titulo, " ", h("span", { class: "cel-entrada-endereco", text: lido.endereco ? `${lido.endereco}?` : "" })),
      avisoDoDesfecho,
      avisoDoEstado,
      !doEstado && !desfecho && h("p", { class: "cel-entrada-nota", text: f.sobre }),
      ficha,
      podeEntrar && h("p", { class: "cel-entrada-nota", text: f.impressaoNota }),
      campo && [h("label", { class: "cel-entrada-campo", for: "cel-entrada-apelido" }, rotulo(f.apelido)), campo],
      podeEntrar && estado === "pronto" && h("p", { class: "cel-entrada-nota", text: f.primeiraEntrada }),
      lido.persistencia && h("p", { class: "cel-entrada-nota", text: PALAVRAS.persistencia[lido.persistencia] }),
    ], acoes);
    if (desfecho?.erro === "apelido-tomado") campo?.focus();
  }

  /**
   * A recusa da ponte enquanto a sessão pelo navegador (a 2.6 da F4) não
   * existe: o `entrar` resolve `recusado` com ela, e a tela diz que a entrada
   * pelo navegador ainda não está pronta, e não um motivo cru.
   */
  const AINDA_NAO_NO_NAVEGADOR = "AindaNaoNoNavegador";

  function fraseDoDesfecho(desfecho) {
    if (desfecho.erro === "recusado" && desfecho.razao === AINDA_NAO_NO_NAVEGADOR) return PALAVRAS.desfechos["sem-identidade"];
    return PALAVRAS.desfechos[desfecho.erro] ?? PALAVRAS.desfechos.recusado;
  }

  function agoraNao() {
    identidade?.agoraNao();
    inicio();
  }

  // --------------------------------------------------------- conectando

  /**
   * A tela de conectar: a única animação da marca, o enlace que se desenha do
   * nó cheio ao vazio (`docs/marca.md`), e embaixo as etapas de hoje, que o
   * desktop escreve em `#boot-etapa` pelo `ConnectStageChanged`. Dura o tempo
   * da conexão, e só.
   */
  async function conectando(endereco, tentar, lido, conhecido = null) {
    const etapa = document.getElementById("boot-etapa");
    const simbolo = await simboloDaMarca();
    mostrar([
      h("div", { class: "cel-entrada-conectando", "aria-busy": "true" },
        simbolo,
        titulo(PALAVRAS.conectando, " ", h("span", { class: "cel-entrada-endereco", text: endereco })),
        etapa),
    ], []);
    let desfecho;
    try {
      desfecho = await tentar();
    } catch (falha) {
      desfecho = { erro: "recusado", razao: String(falha?.message ?? falha) };
    }
    // A etapa volta para a caixa do desktop, que é de onde ela é escrita.
    if (etapa) boot.querySelector(".boot-caixa")?.append(etapa);
    if (desfecho?.ok) return;
    if (desfecho?.erro === "mods-a-aceitar") return aceite(endereco, desfecho, lido);
    if (lido) return entrarEm(identidade.ler() ?? lido, desfecho);
    return inicio(conhecido && desfecho);
  }

  async function entrar(lido, apelido) {
    await conectando(lido.endereco, async () => ({ ...(await identidade.entrar(apelido)), apelido }), lido);
  }

  /** O símbolo inteiro, lido do arquivo da `ui/` para o enlace poder ser desenhado. */
  let simboloLido = null;
  async function simboloDaMarca() {
    if (!simboloLido) {
      try {
        const texto = await (await fetch("marca-simbolo.svg")).text();
        const svg = new DOMParser().parseFromString(texto, "image/svg+xml").documentElement;
        if (svg.nodeName !== "svg") throw new Error(svg.nodeName);
        simboloLido = document.importNode(svg, true);
        simboloLido.setAttribute("aria-hidden", "true");
        simboloLido.removeAttribute("role");
      } catch (falha) {
        console.warn("celular-entrada, marca-simbolo.svg:", falha);
        return h("img", { class: "cel-entrada-simbolo-parado", src: "marca-simbolo.svg", alt: "" });
      }
    }
    return simboloLido.cloneNode(true);
  }

  // ------------------------------------------------------- o aceite de MODs

  /**
   * O aceite do desktop, com a mesma lista e as mesmas frases. O ACEITAR é
   * desviado na captura para o `aceitarOsMods` da identidade: no navegador o
   * sim é gravado por ela, e a volta usa o token que só ela guarda. RECUSAR
   * fecha pelo caminho do desktop e encerra a tentativa.
   */
  /** O desvio do aceite aberto, para o próximo não somar outro por cima. */
  let soltarOAceite = null;

  function aceite(endereco, { mods, conjunto }, lido) {
    soltarOAceite?.();
    const camada = document.getElementById("mods-aceite");
    const aceitar = document.getElementById("mods-aceitar");
    let aceitou = false;
    const aoAceitar = (evento) => {
      evento.stopImmediatePropagation();
      aceitou = true;
      soltarOAceite();
      fecharAceiteDeMods();
      conectando(endereco, () => identidade.aceitarOsMods(conjunto), lido);
    };
    // RECUSAR, Escape e o toque fora fecham pelo caminho do desktop, e os três
    // dizem a mesma coisa: sair da pergunta não entra. Aqui isso encerra a
    // tentativa, venha o fechamento de onde vier.
    const observador = new MutationObserver(() => {
      if (!camada.hidden) return;
      soltarOAceite();
      if (!aceitou) agoraNao();
    });
    soltarOAceite = () => {
      aceitar.removeEventListener("click", aoAceitar, true);
      observador.disconnect();
      soltarOAceite = null;
    };
    aceitar.addEventListener("click", aoAceitar, true);
    // A «ENTRAR EM» fica embaixo do aceite, e é o que se vê quando ele fecha.
    entrarEm(identidade.ler() ?? lido);
    abrirAceiteDeMods(endereco, mods, conjunto, null);
    observador.observe(camada, { attributes: true, attributeFilter: ["hidden"] });
  }

  // ---------------------------------------- a entrada direta (D-celular-3)
  //
  // Com a impressão conferida (a chave de sempre, ou a primeira conferida pelo
  // link), o aperto da tela de entrada do desktop não aparece: a pessoa já
  // conferiu o endereço e a impressão na «ENTRAR EM», e o navegador só abriu a
  // conexão com o certificado que o link anuncia. O passo de entrar roda igual
  // (P11, em `tela-auth.js`), e uma falha nele fica na tela, com a frase. Um
  // veredito diferente continua indo ao aperto.
  //
  // O registro da conexão (DETALHES DA CONEXÃO, um `<details>` fechado no
  // desktop) fica aberto na entrada direta: é nele que o `inserirPlug` escreve
  // a falha, e sem o aperto não há outro lugar na tela onde ela apareça.
  CASCA.entradaDireta = (veredito) => {
    if (!(veredito === "Known" || veredito?.FirstContactVerified)) return null;
    document.querySelector("#tela-auth .auth-aperto").open = true;
    return PALAVRAS.entradaDireta;
  };

  // A espera da portaria para quando a página sai da tela (o `visibilitychange`
  // do `tela-auth.js`), e no celular «enquanto estiver aberta» não basta: uma
  // página aberta atrás de outro app está aberta e parada. A espera diz isso.
  document.querySelector("#tela-auth .auth-espera-fatos")?.append(h("li", { class: "cel-espera-pausa", text: PALAVRAS.esperaPausa }));

  // ------------------------------------------------------------ a ligação

  CELULAR.registrar("entrada", {
    /**
     * Passa a tela para a identidade da página e mostra a entrada: a
     * «ENTRAR EM» do link da carga, quando a página veio com um, ou a tela de
     * início. `conhecidos` e `entrarNoConhecido` são os servidores deste
     * aparelho.
     */
    async usar(daPagina, { conhecidos, entrarNoConhecido, link = null } = {}) {
      identidade = daPagina;
      fontes = {
        conhecidos: conhecidos ?? (identidade.conhecidos ? () => identidade.conhecidos() : async () => []),
        entrarNoConhecido: entrarNoConhecido
          ?? (identidade.entrarNoConhecido ? (impressao, apelido) => identidade.entrarNoConhecido(impressao, apelido) : null),
      };
      // A identidade da página chega com o link da carga já aberto (a F4 o
      // abre, e a casca nunca vê o token do fragmento): a tentativa dela é a
      // «ENTRAR EM»; sem tentativa, a tela de início.
      const lido = link ? await identidade.abrir(link) : identidade.ler();
      return lido ? entrarEm(lido) : inicio();
    },
  });
  inicio();
  // **A identidade da página** é da F4 (`identidadeDaPagina()`, combinada em
  // 10/10): uma instância só, com o `.wasm` e o `conectar` da sessão pelo
  // navegador. Enquanto ela não existe na `identidade-web.js`, a tela fica na de
  // início, e o COLAR diz que a entrada pelo navegador ainda não está pronta.
  if (typeof identidadeDaPagina === "function") {
    identidadeDaPagina().then((daPagina) => CELULAR.ajudante("entrada").usar(daPagina))
      .catch((falha) => console.warn("celular-entrada, identidadeDaPagina:", falha));
  }
})();
