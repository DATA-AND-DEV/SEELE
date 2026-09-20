// As superfícies de um MOD: página, painel, diálogo, popover e aviso.
//
// ADR 0052, §3 do plano. É a resposta a U01, U02 e U03 — os três P1 da
// auditoria de 20/09/2026, e os três a mesma frase dita de ângulos diferentes:
//
//   «Todos os MODs disputam uma faixa de até 240 px.»
//   «A rolagem pertence ao contêiner dos três MODs.»
//   «Não há caminho de MOD para abrir uma experiência ampla.»
//
// # Por que uma superfície não é uma região maior
//
// Aumentar a faixa resolveria o primeiro sintoma e nenhum dos três problemas.
// O §15 do plano diz isso pelo nome: «Aumentar `max-height` da faixa não cria
// uma API de aplicações.»
//
// O que faltava era **ciclo de vida**. Uma região existe enquanto o MOD estiver
// de pé, aparece sozinha ao conectar e não tem como ser aberta, fechada, ter
// foco, ter título, ter rota ou ter uma saída garantida. Uma superfície tem as
// seis coisas, e é por isso que ela pode ser uma mesa de jogo e a região não.
//
// # A garantia que o produto dá, e o MOD não pode tirar
//
// Toda superfície tem uma saída montada **pelo produto**: um botão que este
// arquivo cria, com o texto que este arquivo escreve, ligado a um comando que
// este arquivo controla. Um MOD que não desenhe botão nenhum, que trave no meio
// da montagem ou que sature a fila continua sendo uma superfície de onde se
// sai.
//
// §6: «Personalização estética pode substituir a faixa de sinal; não pode
// falsificar autorização, bloquear saída ou encobrir uma confirmação de
// confiança do produto.»

/**
 * Torna inerte tudo **menos o ramo que contém** um nó, e devolve o que mudou.
 *
 * # O defeito que esta função existe para não ter
 *
 * A versão anterior percorria os filhos de `document.body` e excetuava apenas
 * `palco-de-camadas`. No `index.html` do aplicativo esse palco é filho de
 * `section#tela-sessao` — a ancestralidade é `html > body > section#tela-sessao
 * > div#palco-de-camadas` —, então o laço marcava `tela-sessao` como inerte e
 * **o diálogo descendia dela**. `inert` é herdado: o modal inteiro ficava sem
 * foco e sem clique, exatamente o contrário do que ele queria fazer.
 *
 * A revisão de 20/09/2026 reproduziu isso com o método real. O laboratório não
 * pegava porque ele põe o palco diretamente no `body`: os mesmos arquivos de
 * renderer, numa hierarquia diferente, não exercitam a integração.
 *
 * # Como ela funciona
 *
 * Sobe do nó até o `body` e, em cada nível, torna inertes **os irmãos** —
 * nunca o ancestral. O ramo que leva até a camada fica intocado, e tudo o que
 * está fora dele para de responder.
 *
 * # O estado anterior é preservado
 *
 * Um nó que já era inerte antes não entra na lista, e por isso não é
 * despertado no fim. Dois modais abertos em ordem — o de um MOD e a
 * confirmação de descarte do produto por cima — se desfazem em qualquer ordem
 * sem um acordar o que o outro adormeceu.
 *
 * @param {Element} dentro O nó que continua alcançável.
 * @returns {Element[]} O que **esta** chamada tornou inerte.
 */
function inertarFora(dentro) {
  const mudados = [];
  if (!dentro) return mudados;
  let ramo = dentro;
  while (ramo && ramo.parentNode && ramo.parentNode !== document) {
    const pai = ramo.parentNode;
    for (const irmao of Array.from(pai.children ?? [])) {
      // O ramo ativo nunca: tornar o próprio ancestral inerte é tornar o
      // diálogo inerte, porque `inert` desce.
      if (irmao === ramo || irmao.inert) continue;
      irmao.inert = true;
      mudados.push(irmao);
    }
    ramo = pai;
  }
  return mudados;
}

/** Os tipos de superfície, e o que cada um exige do host. */
const TIPOS_DE_SUPERFICIE = Object.freeze({
  pagina: { host: "palco-de-paginas", foco: false, camada: false },
  painel: { host: "palco-de-paineis", foco: false, camada: false },
  dialogo: { host: "palco-de-camadas", foco: true, camada: true },
  aviso: { host: "avisos-de-mod", foco: false, camada: false },
});

/** Quantas superfícies um MOD mantém de pé ao mesmo tempo. */
const TETO_DE_SUPERFICIES = 12;
/** Quantos avisos cabem na fila antes de os mais velhos saírem. */
const TETO_DE_AVISOS = 4;

/**
 * Uma superfície montada, com o renderer dela e os recursos que ela segura.
 *
 * O dono é o mesmo par `(instancia, geracao)` da região, e pela mesma razão: um
 * MOD recarregado dentro da sessão não herda as janelas da instância anterior.
 */
class SuperficieDeMod {
  /**
   * @param {object} dono O que `donoDaRegiao` devolve, mais `escolherArquivo`.
   * @param {object} descricao O que o MOD declarou.
   * @param {object} palcos `{ paginas, paineis, camadas, avisos }` do produto.
   */
  constructor(id, dono, descricao, palcos) {
    this.id = id;
    this.dono = dono;
    this.palcos = palcos;
    this.chave = String(descricao?.id ?? "").slice(0, 64) || "sem-id";
    this.tipo = Object.hasOwn(TIPOS_DE_SUPERFICIE, descricao?.tipo) ? descricao.tipo : "dialogo";
    this.modal = this.tipo === "dialogo" ? descricao?.modal !== false : false;
    this.titulo = String(descricao?.titulo ?? "").slice(0, 120);
    this.descricaoCurta = String(descricao?.descricao ?? "").slice(0, 300);
    this.contexto = descricao?.contexto ?? null;
    this.lado = descricao?.lado === "esquerda" ? "esquerda" : "direita";
    this.focoInicial = String(descricao?.focoInicial ?? "").slice(0, 120);
    this.aoFecharComAlteracoes = descricao?.fecharComAlteracoes === "confirmar"
      ? "confirmar"
      : "fechar";
    this.suja = false;
    this.visivel = descricao?.visivel !== false;
    this.solta = false;
    /** Quem tinha o foco quando esta superfície abriu, para devolvê-lo. */
    this.focoAnterior = null;
    /** O que ficou inerte por causa de um modal, para deixar de ficar. */
    this.inertes = [];

    this.montarCasca(descricao);
    this.renderer = new RegiaoDeMod(id, dono, this.corpo, PERFIS_DE_RENDER.superficie);
    // Registrada **na criação**: uma superfície criada e não registrada é uma
    // janela que a saída não encontra.
    dono.instancia?.registrar(`${id}: a superfície ${this.chave}`, () => this.descartar());
  }

  /**
   * A casca que o produto monta: cabeçalho, corpo, rodapé e a saída.
   *
   * Nada aqui vem do MOD além de texto, e texto entra por `textContent`. O
   * título de uma superfície é o nome acessível dela — o que um leitor de tela
   * anuncia ao entrar —, e por isso ele existe mesmo quando o MOD não deu um.
   */
  montarCasca(descricao) {
    const eDialogo = this.tipo === "dialogo";
    if (eDialogo) {
      this.camada = elemento("div", "camada-de-mod");
      this.camada.dataset.modal = this.modal ? "sim" : "nao";
      this.raiz = elemento("div", "superficie-de-mod dialogo-de-mod");
      this.raiz.setAttribute("role", this.modal ? "dialog" : "dialog");
      if (this.modal) this.raiz.setAttribute("aria-modal", "true");
      // **O tamanho pedido entra como token, e o CSS decide o que cabe.** Um
      // MOD pode pedir 2000 px; `max-width: min(pedido, 100vw - 32px)` é o que
      // impede que o fechamento fique fora da tela.
      const largura = Math.min(Math.max(Number(descricao?.tamanho?.largura) || 560, 280), 1600);
      this.raiz.style.setProperty("--largura-pedida", `${largura}px`);
      this.camada.append(this.raiz);
    } else if (this.tipo === "pagina") {
      this.raiz = elemento("section", "superficie-de-mod pagina-de-mod");
      this.raiz.setAttribute("role", "region");
    } else if (this.tipo === "painel") {
      this.raiz = elemento("aside", "superficie-de-mod painel-de-mod");
      this.raiz.dataset.lado = this.lado;
      const largura = Math.min(Math.max(Number(descricao?.tamanho?.largura) || 300, 200), 640);
      this.raiz.style.setProperty("flex", `0 0 ${largura}px`);
    } else {
      this.raiz = elemento("div", "superficie-de-mod aviso-de-mod");
      this.raiz.setAttribute("role", "status");
      this.raiz.dataset.tom = descricao?.tom === "erro" ? "erro" : "normal";
    }
    this.raiz.dataset.mod = this.id;
    this.raiz.dataset.superficie = this.chave;

    const nome = this.titulo || `Conteúdo de ${this.id}`;
    this.raiz.setAttribute("aria-label", nome);

    if (this.tipo === "aviso") {
      this.corpo = this.raiz;
      return;
    }

    const prefixo = this.tipo === "pagina" ? "pagina-de-mod"
      : this.tipo === "painel" ? "painel-de-mod" : "dialogo-de-mod";
    const cabeca = elemento("header", `${prefixo}-cabecalho`);
    const titulo = elemento(this.tipo === "painel" ? "h3" : "h2", `${prefixo}-titulo`, nome);
    cabeca.append(titulo);
    this.tituloNo = titulo;

    // **De quem é esta tela.** Um MOD pode desenhar o que quiser aqui dentro,
    // inclusive algo que se pareça com o produto. Quem está olhando tem direito
    // de saber de quem é a janela, e essa linha é do produto.
    const origem = elemento("span", `${prefixo}-origem`, this.id);
    cabeca.append(origem);

    // A saída. Montada aqui, e não pelo MOD: ver o cabeçalho deste arquivo.
    const sair = elemento("button", "superficie-de-mod-sair", this.tipo === "pagina" ? "VOLTAR" : "FECHAR");
    sair.type = "button";
    sair.setAttribute("aria-label", this.tipo === "pagina"
      ? "Voltar à conversa"
      : `Fechar ${nome}`);
    this.aoSair = () => this.pedirFechamento("saida-do-produto");
    sair.addEventListener("click", this.aoSair);
    this.botaoDeSaida = sair;
    cabeca.append(sair);

    this.corpo = elemento("div", `${prefixo}-corpo`);
    this.raiz.append(cabeca, this.corpo);

    if (eDialogo) {
      this.rodape = elemento("footer", "dialogo-de-mod-rodape");
      this.rodape.hidden = true;
      this.raiz.append(this.rodape);
    }
  }

  /** Põe a superfície no palco dela e, sendo modal, prende o foco. */
  abrir() {
    if (this.solta) return;
    const alvo = this.tipo === "dialogo" ? this.palcos.camadas
      : this.tipo === "pagina" ? this.palcos.paginas
        : this.tipo === "painel" ? this.palcos.paineis
          : this.palcos.avisos;
    if (!alvo) return;
    const no = this.tipo === "dialogo" ? this.camada : this.raiz;
    if (no.parentNode !== alvo) alvo.append(no);
    alvo.hidden = false;
    this.aplicarVisibilidade();

    if (this.tipo === "dialogo" && this.modal) this.prenderFoco();
    else if (this.tipo === "pagina") this.focarPrimeiro();
  }

  /**
   * Foco contido, Escape que pede a saída, e o retorno ao acionador.
   *
   * Implementado sobre o padrão de diálogo do APG da WAI-ARIA, que o §3 do
   * plano cita. O que o padrão não diz e este produto exige está no comentário
   * de `pedirFechamento`: o MOD não pode vetar indefinidamente a saída.
   */
  prenderFoco() {
    this.focoAnterior = document.activeElement;

    // O resto da aplicação fica inerte de verdade — `inert`, e não só escuro.
    // Escurecer sem tornar inerte é a armadilha clássica: parece modal e
    // responde a Tab.
    this.inertes = inertarFora(this.camada);

    this.aoTeclar = (evento) => {
      if (evento.key === "Escape") {
        evento.preventDefault();
        this.pedirFechamento("escape");
        return;
      }
      if (evento.key !== "Tab") return;
      const focaveis = this.focaveis();
      if (!focaveis.length) return;
      const primeiro = focaveis[0];
      const ultimo = focaveis[focaveis.length - 1];
      if (evento.shiftKey && document.activeElement === primeiro) {
        evento.preventDefault();
        ultimo.focus();
      } else if (!evento.shiftKey && document.activeElement === ultimo) {
        evento.preventDefault();
        primeiro.focus();
      }
    };
    this.raiz.addEventListener("keydown", this.aoTeclar);
    this.focarPrimeiro();
  }

  /** O que dá para focar aqui dentro, na ordem em que aparece. */
  focaveis() {
    return Array.from(this.raiz.querySelectorAll(
      "button:not(:disabled),input:not(:disabled),select:not(:disabled),"
      + "textarea:not(:disabled),[href],[tabindex]:not([tabindex=\"-1\"])",
    )).filter((no) => no.offsetParent !== null || no === document.activeElement);
  }

  /**
   * Põe o foco onde o MOD pediu, ou no primeiro controle, ou na saída.
   *
   * A última alternativa é a que importa: uma superfície sem controle nenhum
   * — porque o MOD ainda não desenhou, ou porque ele falhou — precisa receber
   * o foco em algum lugar, senão Tab começa fora dela.
   */
  focarPrimeiro() {
    const pedido = this.focoInicial
      ? this.corpo.querySelector(`[data-chave-do-mod="${CSS.escape(this.focoInicial)}"] input,`
        + `[data-chave-do-mod="${CSS.escape(this.focoInicial)}"] textarea,`
        + `[data-chave-do-mod="${CSS.escape(this.focoInicial)}"] select,`
        + `[data-chave-do-mod="${CSS.escape(this.focoInicial)}"]`)
      : null;
    const destino = pedido ?? this.focaveis()[0] ?? this.botaoDeSaida ?? this.raiz;
    // `requestAnimationFrame` porque o nó acabou de entrar no documento e ainda
    // pode não ter layout — e sem layout `focus()` não faz nada.
    requestAnimationFrame(() => {
      if (!this.solta) destino?.focus?.();
    });
  }

  /** O que o MOD declarou, montado aqui dentro. */
  montar(conteudo) {
    if (this.solta) return 0;
    let recusados = this.renderer.aplicar(conteudo);
    // O rodapé fixo do diálogo: o que o MOD pôr em `acoes` com `fixas: true`
    // sobe para cá, porque salvar/cancelar não podem sumir numa rolagem longa.
    if (this.rodape) recusados += this.recolherAcoesFixas();
    return recusados;
  }

  /**
   * Move para o rodapé as ações que o MOD declarou como fixas.
   *
   * Move o **nó já montado**, em vez de montá-lo duas vezes: montar de novo
   * criaria um segundo botão com os mesmos ouvintes, e o clique chegaria ao
   * MOD duas vezes.
   */
  recolherAcoesFixas() {
    const fixas = this.corpo.querySelector("[data-forma=\"acoes\"][data-fixas=\"sim\"]");
    if (!fixas) {
      if (!this.rodape.hidden) {
        this.rodape.replaceChildren();
        this.rodape.hidden = true;
      }
      return 0;
    }
    if (fixas.parentNode !== this.rodape) {
      this.rodape.replaceChildren(fixas);
      this.rodape.hidden = false;
    }
    return 0;
  }

  /** As classes que esta superfície oferece aos nós dela. */
  declararClasses(classes) {
    return this.renderer.declararClasses(classes);
  }

  /** Estado sujo: muda o que fechar significa. */
  marcarSuja(suja) {
    this.suja = suja === true;
  }

  aplicarVisibilidade() {
    const escondida = !this.visivel;
    const no = this.tipo === "dialogo" ? this.camada : this.raiz;
    if (no.hidden !== escondida) no.hidden = escondida;
    // **Suspende o desenho do que não se vê.** §10: «Suspender desenho,
    // animação e consulta de regiões invisíveis.» A folha lê este atributo.
    this.raiz.dataset.suspensa = escondida ? "sim" : "nao";
  }

  /**
   * Mostra — e **remonta**, quando ela tinha sido fechada.
   *
   * # O contrato que estava quebrado
   *
   * R6 da revisão de 20/09/2026: «`fechar` remove o nó do palco; `mostrar` só
   * altera flags/hidden e não o reinsere. A sequência real de métodos termina
   * com superfície dita visível, ainda fora do documento.»
   *
   * O MOD recebia sucesso e a janela não aparecia. Reabrir por `criar` com o
   * mesmo `id` funcionava — ele chama `abrir` —, mas isso não torna o método
   * público correto: um MOD que guarde o punho e chame `mostrar` estava num
   * caminho que sempre mentiu.
   *
   * `abrir` é idempotente: ele só reinsere quando o nó não está no palco, e
   * reprende o foco só quando há foco a prender.
   */
  mostrar() {
    if (this.solta) throw new Error(`a superfície «${this.chave}» foi descartada`);
    this.visivel = true;
    this.abrir();
  }

  /**
   * Oculta — e **solta o que um modal prendeu**.
   *
   * A outra metade de R6: «`ocultar` de um modal não libera o foco nem o
   * estado inerte da aplicação.» Uma superfície invisível com a aplicação
   * inerte atrás dela é uma janela travada sem nada na tela para explicar.
   *
   * Ela continua montada, e por isso `mostrar` a traz de volta sem recriar
   * nada — é a diferença entre ocultar e fechar.
   */
  ocultar() {
    if (this.solta) return;
    this.visivel = false;
    this.soltarFoco();
    this.aplicarVisibilidade();
  }

  /**
   * Alguém pediu para fechar — a saída do produto, Escape, ou o próprio MOD.
   *
   * **O MOD é avisado e pode responder; ele não pode recusar para sempre.**
   * §3: «Se há alterações não salvas, o host apresenta a decisão de
   * salvar/descartar; o MOD não pode vetar indefinidamente a saída da sessão.»
   *
   * A regra concreta: com `fecharComAlteracoes: 'confirmar'` e estado sujo, o
   * primeiro pedido vira um evento e uma confirmação do **produto**; o segundo
   * fecha. Sem estado sujo, fecha na hora. A saída da sessão não passa por
   * aqui — ela chama `descartar`, que não pergunta nada.
   */
  pedirFechamento(porque) {
    if (this.solta) return;
    if (this.suja && this.aoFecharComAlteracoes === "confirmar" && porque !== "confirmado") {
      this.dono.falar({
        nome: "fechar-pedido",
        superficie: this.chave,
        porque,
        temAlteracoes: true,
      });
      // A confirmação é do produto, e não do MOD: um MOD que não responda ao
      // evento acima não pode deixar a pessoa presa numa janela.
      this.confirmarDescarte(porque);
      return;
    }
    this.dono.falar({ nome: "fechar", superficie: this.chave, porque });
    this.fechar();
  }

  /**
   * A pergunta que o produto faz, com as palavras do produto.
   *
   * Pela caixa de confirmação que a moderação já usa — a mesma que apaga uma
   * Linha —, e não por uma caixa nova: quem já viu aquela caixa sabe o que ela
   * significa, e um segundo desenho para a mesma pergunta é uma segunda coisa
   * para aprender.
   *
   * O texto diz **a consequência**, e não a ação: «o que você escreveu e não
   * gravou será perdido» responde à pergunta que a pessoa tem.
   */
  confirmarDescarte(porque) {
    const nome = this.titulo || this.id;
    const consequencia = `O que você escreveu em «${nome}» e não gravou será perdido. `
      + "Cancelar aqui mantém a janela aberta com as alterações.";
    if (typeof abrirConfirmacao !== "function") {
      // Sem a camada de moderação carregada não há como perguntar — e não
      // perguntar é melhor do que prender alguém numa janela que não fecha.
      this.dono.falar({ nome: "fechar", superficie: this.chave, porque, descartou: true });
      this.fechar();
      return;
    }
    // **A camada do produto precisa estar alcançável.** Ela é um irmão do ramo
    // que `inertarFora` adormeceu, e um diálogo inerte é uma pergunta que não
    // se pode responder. A revisão de 20/09/2026 pediu as duas coisas: acima
    // (ver o `z-index` em `camada-moderar.css`) e interativa.
    //
    // Acordada só enquanto a pergunta está de pé, e devolvida ao estado
    // anterior nos dois desfechos. `#moderar` é o elemento porque é ele que a
    // moderação mostra; acordar o pai inteiro devolveria a aplicação toda.
    const confirmacao = document.getElementById("moderar");
    const dormia = Boolean(confirmacao && this.inertes.includes(confirmacao));
    if (dormia) confirmacao.inert = false;
    const readormecer = () => {
      if (dormia && confirmacao && !this.solta) confirmacao.inert = true;
    };

    abrirConfirmacao("FECHAR SEM GRAVAR?", consequencia, "DESCARTAR E FECHAR", () => {
      readormecer();
      if (this.solta) return;
      this.dono.falar({ nome: "fechar", superficie: this.chave, porque, descartou: true });
      this.fechar();
    }, readormecer);
  }

  /**
   * Tira do palco, devolve o foco, e mantém o renderer para reabrir.
   *
   * O nó sai do documento e a superfície continua existindo: o conteúdo
   * montado, as classes e o que estava escrito nos campos permanecem, e
   * `mostrar` os traz de volta. `descartar` é o outro verbo — esse acaba com
   * ela.
   */
  fechar() {
    if (this.solta) return;
    this.visivel = false;
    this.soltarFoco();
    const no = this.tipo === "dialogo" ? this.camada : this.raiz;
    no.remove();
    this.aoPalcoMudar?.();
  }

  /** Ela está no documento agora? É o que separa fechada de oculta. */
  get montada() {
    if (this.solta) return false;
    const no = this.tipo === "dialogo" ? this.camada : this.raiz;
    return Boolean(no?.parentNode);
  }

  /** Solta o que o modal prendeu, e devolve o foco a quem o tinha. */
  soltarFoco() {
    for (const irmao of this.inertes) irmao.inert = false;
    this.inertes.length = 0;
    if (this.aoTeclar) {
      this.raiz.removeEventListener("keydown", this.aoTeclar);
      this.aoTeclar = null;
    }
    // **O foco volta ao acionador, ou a um destino válido.** Devolvê-lo a um
    // nó que saiu do documento é o mesmo que não devolvê-lo: o foco cai no
    // `<body>` e Tab recomeça do topo da aplicação.
    const anterior = this.focoAnterior;
    this.focoAnterior = null;
    if (anterior && anterior.isConnected && typeof anterior.focus === "function") {
      anterior.focus();
    }
  }

  /**
   * Acaba com a superfície e solta tudo.
   *
   * Idempotente, como `RegiaoDeMod.soltar`: a saída solta as superfícies e o
   * encerramento da instância solta os recursos dela, e os dois caminhos
   * chegam aqui.
   */
  descartar() {
    if (this.solta) return;
    this.solta = true;
    this.soltarFoco();
    this.botaoDeSaida?.removeEventListener("click", this.aoSair);
    this.renderer.soltar();
    const no = this.tipo === "dialogo" ? this.camada : this.raiz;
    no.remove();
    this.aoPalcoMudar?.();
  }
}

/**
 * As superfícies de uma instância de MOD, e os tetos delas.
 *
 * Uma por instância e não uma por MOD, pelo motivo de sempre: um MOD
 * recarregado dentro da mesma sessão não herda as janelas da instância anterior.
 */
class SuperficiesDoMod {
  constructor(id, dono, palcos) {
    this.id = id;
    this.dono = dono;
    this.palcos = palcos;
    this.porChave = new Map();
    /** Os avisos de pé, em ordem de chegada, para a fila ter fim. */
    this.avisos = [];
  }

  /** Cria (ou reaproveita) uma superfície e a abre. */
  criar(descricao) {
    const chave = String(descricao?.id ?? "").slice(0, 64) || "sem-id";
    const existente = this.porChave.get(chave);
    if (existente && !existente.solta) {
      // **Reabrir não recria.** Um MOD que chame `criar` de novo com o mesmo
      // `id` está dizendo «mostre aquela»; recriar jogaria fora o que estava
      // escrito nos campos dela.
      existente.mostrar();
      existente.abrir();
      return { superficie: chave, reaproveitada: true };
    }
    if (this.porChave.size >= TETO_DE_SUPERFICIES) {
      throw new Error(`um MOD mantém até ${TETO_DE_SUPERFICIES} superfícies de pé`);
    }
    const superficie = new SuperficieDeMod(this.id, this.dono, descricao, this.palcos);
    superficie.aoPalcoMudar = () => this.arrumarPalcos();
    this.porChave.set(chave, superficie);
    superficie.abrir();
    this.arrumarPalcos();
    if (superficie.tipo === "aviso") this.enfileirarAviso(superficie);
    return { superficie: chave, reaproveitada: false };
  }

  /**
   * A fila de avisos tem fim, e o fim é visível.
   *
   * §3: «Fila limitada, duração e histórico de erros; sem tempestade de
   * avisos.» Um MOD em laço de erro produziria um aviso por volta, e a tela de
   * quem conversa viraria a tela dele.
   */
  enfileirarAviso(superficie) {
    this.avisos.push(superficie);
    while (this.avisos.length > TETO_DE_AVISOS) {
      this.avisos.shift()?.descartar();
    }
  }

  de(chave) {
    return this.porChave.get(String(chave)) ?? null;
  }

  fechar(chave, porque = "pedido-do-mod") {
    this.de(chave)?.pedirFechamento(porque === "descartar" ? "confirmado" : porque);
  }

  descartar(chave) {
    const superficie = this.de(chave);
    if (!superficie) return;
    superficie.descartar();
    this.porChave.delete(String(chave));
    this.avisos = this.avisos.filter((a) => a !== superficie);
    this.arrumarPalcos();
  }

  /** Solta tudo. A saída da sessão passa por aqui. */
  soltarTudo() {
    for (const [chave] of Array.from(this.porChave)) this.descartar(chave);
    this.porChave.clear();
    this.avisos.length = 0;
  }

  /**
   * Esconde os palcos vazios.
   *
   * Um palco de painéis vazio mas visível é uma coluna de 1 px que empurra a
   * conversa — e foi assim que a faixa dos MODs começou.
   */
  arrumarPalcos() {
    for (const palco of Object.values(this.palcos)) {
      if (!palco) continue;
      const vazio = !palco.querySelector(":scope > *:not([hidden])");
      if (palco.hidden !== vazio) palco.hidden = vazio;
    }
  }
}
