// A região de um MOD: o que ele declara, o que a janela monta, e o que ela
// tem de soltar depois.
//
// ADR 0049. O MOD **declara** e o produto monta — com `createElement` e
// `textContent`, nunca com HTML de texto. Este arquivo é a gramática inteira
// dessa declaração, e o registro dos recursos que montá-la cria.
//
// # Por que não em `base.js`
//
// Pela mesma razão de `mods-runtime.js`: ele declara a classe que `base.js`
// constrói, e não chama nada de lá ao carregar. A gramática também é a parte
// que mais cresce quando a API cresce, e ela tem um assunto só.
//
// # Os três compromissos
//
// **Dono.** Cada recurso nasce amarrado a uma instância e a uma geração. Um
// `<audio>` sem dono é um som que continua tocando depois de a pessoa sair, e
// foi assim que a sonda E1 descreveu o pior caso.
//
// **Limite.** Cada coisa que um MOD pode pedir muitas vezes tem teto, e o teto
// é conferido na montagem — não na pintura. Uma árvore de dez mil nós custa na
// hora em que vira DOM.
//
// **Descarte.** Tudo o que é criado é registrado com o que o solta, e soltar é
// a mesma função para «o MOD tirou isto da tela» e para «a sessão acabou».

/**
 * Os tetos da região, num lugar só.
 *
 * Números e não sensibilidades: cada um deles é o ponto em que um MOD deixa de
 * estar usando a API e passa a estar usando a memória de quem está na conversa.
 */
const LIMITES_DA_REGIAO = Object.freeze({
  /** Nós na árvore inteira, somando todos os níveis. */
  nos: 512,
  /** Níveis de fundura. Sem teto, a recursão estoura dentro do produto. */
  fundura: 8,
  /** Campos editáveis por região. */
  campos: 32,
  /** Telas de desenho por região. */
  telas: 4,
  /** Mídias por região — som e imagem contam juntas. */
  midias: 4,
  /** Traços numa tela. */
  tracos: 256,
  /** Figuras declaradas numa tela. */
  figuras: 256,
  /** Pontos num traço. */
  pontos: 512,
  /** Lado máximo de uma tela, em pixels de layout. */
  ladoDaTela: 1024,
  /** Caracteres num nó de texto. */
  texto: 4096,
  /** Caracteres no valor de um campo. */
  valorDoCampo: 1024,
  /** Opções numa escolha. */
  opcoes: 64,
  /** Bytes de mídia somados nesta região. */
  bytesDeMidia: 4 * 1024 * 1024,
});

/**
 * Os tetos de um cartão na lista de pessoas do produto.
 *
 * Separados dos da região, e por um motivo que não é organização: um cartão é
 * desenhado **dentro de uma tela do produto**, ao lado do nome de uma pessoa,
 * e há um por pessoa. Os mesmos 512 nós da região valeriam por cartão — uma
 * lista de vinte pessoas viraria dez mil nós dentro da lateral que o produto
 * usa para dizer quem está falando.
 *
 * O teto de nós é apertado de propósito: um cartão é um retrato, um nome e
 * duas linhas. O que não couber em vinte e quatro nós é uma região, e a região
 * já existe.
 */
const LIMITES_DO_CARTAO = Object.freeze({
  /** Pessoas com cartão, por MOD. */
  cartoes: 64,
  /**
   * Nós num cartão, somando todos os níveis.
   *
   * **Vinte e quatro viraram sessenta e quatro**, e a mudança tem causa. O
   * número antigo saiu de «um cartão é um retrato, um nome e duas linhas» —
   * uma descrição correta de um cartão que **acrescenta** conteúdo a uma linha
   * do produto. U27 disse que isso é insuficiente: «Cartões na API são
   * conteúdo adicional, sem interação, sem composição de banner e sem
   * substituição dos campos nativos. Isso é inferior ao PERFIS anterior.»
   *
   * Um cartão que **substitui** a identidade precisa de faixa, retrato
   * sobreposto, nome, pronomes, status e uma linha de distintivos. Isso é uma
   * caixa com quatro filhos e duas subcaixas — vinte e quatro nós não chegam
   * lá, e o pacote descobria isso como uma recusa sem explicação.
   *
   * Sessenta e quatro continua sendo um teto: uma lista de vinte pessoas dá
   * 1280 nós na lateral, que é o mesmo custo que uma região e meia da API 3.
   */
  nos: 64,
  /** Níveis de fundura dentro de um cartão. */
  fundura: 6,
  /** Mídias somadas em todos os cartões deste MOD. */
  midias: 64,
  /** Bytes de mídia somados em todos os cartões deste MOD. */
  bytesDeMidia: 8 * 1024 * 1024,
});

/**
 * Os perfis de renderização: quem desenha onde, com que teto e que formas.
 *
 * Três lugares, três orçamentos, e a diferença entre eles não é organização:
 *
 * - a **região** é a faixa da API 3, e continua existindo para os pacotes que
 *   já estão publicados. Ela não cresceu: crescer a faixa era exatamente o que
 *   o §15 do plano diz não confundir com uma API de aplicações;
 * - o **cartão** é desenhado ao lado do nome de uma pessoa, e há um por pessoa.
 *   Ele cresceu de 24 para 64 nós porque U27 pediu que ele pudesse
 *   **substituir** a identidade — faixa, retrato sobreposto, nome, pronomes,
 *   status —, e 24 nós não desenham isso. Ganhou também um ponto de clique,
 *   pelo mesmo motivo, e o cuidado de antes vira regra em `montarCartao`;
 * - a **superfície** é uma página, um painel ou um diálogo, e é a única que
 *   tem a área de uma aplicação. O teto dela é o que uma ficha de RPG e um
 *   tabuleiro exigem — e continua sendo teto.
 */
const PERFIS_DE_RENDER = Object.freeze({
  regiao: Object.freeze({
    nome: "regiao",
    nos: 512, fundura: 8, campos: 32, telas: 4, midias: 4,
    bytesDeMidia: 4 * 1024 * 1024,
    formas: null,
  }),
  cartao: Object.freeze({
    nome: "cartao",
    // Os números vêm de `LIMITES_DO_CARTAO`, que os explica um a um. Repeti-los
    // aqui seria criar a segunda cópia que discorda no dia em que uma mudar.
    nos: LIMITES_DO_CARTAO.nos,
    fundura: LIMITES_DO_CARTAO.fundura,
    campos: 0,
    telas: 0,
    midias: LIMITES_DO_CARTAO.midias,
    bytesDeMidia: LIMITES_DO_CARTAO.bytesDeMidia,
    formas: Object.freeze(new Set([
      "texto", "titulo", "linha", "lista", "item", "midia",
      "caixa", "pilha", "grade", "separador", "espaco",
      "retrato", "distintivo",
    ])),
  }),
  superficie: Object.freeze({
    nome: "superficie",
    nos: 4096, fundura: 16, campos: 128, telas: 4, midias: 24,
    bytesDeMidia: 24 * 1024 * 1024,
    formas: null,
  }),
});

/**
 * As formas que um cartão aceita — e por que as outras continuam de fora.
 *
 * A lista está em `PERFIS_DE_RENDER.cartao.formas`, e cresceu com a API 4:
 * `caixa`, `pilha`, `grade`, `separador`, `espaco`, `retrato` e `distintivo`
 * entraram. Não é um afrouxamento — é a diferença entre acrescentar uma linha
 * embaixo do nome e **desenhar a identidade**, que é o que U27 pediu.
 *
 * **O que continua de fora é tudo que receba foco.** A razão de antes não
 * mudou: a linha do roster já tem um botão do produto — o nome, que abre a
 * moderação —, e um `botao` de MOD ao lado dele dividiria a ordem de tabulação
 * e a área de toque de um controle do produto com um terceiro. `campo`,
 * `escolha`, `textoLongo`, `numero`, `deslizante`, `marca`, `interruptor`,
 * `cor`, `link`, `arquivo`, `formulario` e `abas` ficam fora por isso.
 *
 * `tela` fica de fora por outra: um canvas com arraste dentro de uma lista que
 * rola é o arraste do MOD brigando com a rolagem de quem lê.
 *
 * **Clicar o cartão inteiro é outra coisa, e ela existe.** A substituição de
 * `pessoa.cartao` declara uma ação principal, e quem monta o alvo de clique é
 * o produto — um botão só, com o nome acessível que ele escreve, indo para o
 * comando que ele controla. Ver `mods-contribuicoes.js`.
 */
const FORMAS_DO_CARTAO = Object.freeze(PERFIS_DE_RENDER.cartao.formas);

/**
 * As formas que a API conhece, e a etiqueta que cada uma vira.
 *
 * # O que a API 4 acrescentou, e por quê
 *
 * A lista da API 3 tinha onze formas e descrevia bem um **formulário**: texto,
 * título, lista, campo, escolha, botão. A auditoria de 20/09/2026 mostrou o
 * custo disso — «editar perfil/tema e jogar exige rolar um rodapé» —, e o
 * diagnóstico dela não foi que faltava espaço: foi que faltava **composição**.
 * Uma linha de perfil com retrato sobreposto a uma faixa não é um formulário
 * mais largo; é uma caixa dentro de outra, com posição, recorte e camada.
 *
 * As formas novas são de três tipos:
 *
 * - **composição** (`caixa`, `pilha`, `grade`, `rolagem`, `separador`,
 *   `espaco`): o que permite montar componentes próprios em vez de pedir um
 *   componente novo ao SEELE a cada combinação visual;
 * - **controle** (`textoLongo`, `numero`, `deslizante`, `marca`,
 *   `interruptor`, `cor`, `abas`, `formulario`, `acoes`): o que o §5 do plano
 *   chama de capacidade mínima, e que a auditoria nomeou uma a uma — biografia
 *   multilinha (U20), seletor de cor com amostra (U23), abas na gestão (U12);
 * - **apresentação de pessoa** (`retrato`, `distintivo`, `link`): o que U27
 *   pediu para o PERFIS poder **substituir** a identidade em vez de escrever
 *   uma linha embaixo dela.
 *
 * Toda forma nova é uma decisão de API registrada aqui. Uma forma que não está
 * nesta tabela continua sendo recusada e contada — não vira `div` por
 * conveniência.
 */
const FORMAS_DA_REGIAO = Object.freeze({
  // ---- API 3 ----
  texto: "p",
  titulo: "h3",
  linha: "div",
  lista: "ul",
  item: "li",
  campo: "label",
  escolha: "label",
  botao: "button",
  arquivo: "button",
  tela: "canvas",
  midia: "figure",
  // ---- composição (API 4) ----
  caixa: "div",
  pilha: "div",
  grade: "div",
  rolagem: "div",
  separador: "hr",
  espaco: "div",
  // ---- controle (API 4) ----
  formulario: "form",
  acoes: "div",
  abas: "div",
  aba: "section",
  textoLongo: "label",
  numero: "label",
  deslizante: "label",
  marca: "label",
  interruptor: "label",
  cor: "label",
  // ---- apresentação (API 4) ----
  retrato: "span",
  distintivo: "span",
  link: "button",
});


/** A cor de uma figura, na mesma forma que o tema aceita. */
const COR_DA_FIGURA = /^#[0-9a-f]{6}$/i;

/** As que criam recurso, e por isso passam por contador próprio. */
const FORMAS_COM_TETO = Object.freeze({
  campo: "campos",
  escolha: "campos",
  tela: "telas",
  midia: "midias",
  // Os controles da API 4 contam no mesmo bolso dos campos: são todos caixas
  // de edição com estado, e o teto existe para o custo delas, não para o nome.
  textoLongo: "campos",
  numero: "campos",
  deslizante: "campos",
  marca: "campos",
  interruptor: "campos",
  cor: "campos",
  retrato: "midias",
});

/**
 * As formas cujos filhos o MOD declara.
 *
 * As outras — campo, tela, mídia — **são donas do que têm dentro**: a caixa e o
 * rótulo de um campo, o tocador de uma mídia. Reconciliar `dentro` nelas
 * apagava exatamente esses nós, porque a declaração não os menciona e a
 * reconciliação tira o que sobra. O campo nascia sem caixa, e sem caixa não há
 * o que focar — foi a bancada da região que pegou, na primeira execução.
 */
const FORMAS_COM_FILHOS = Object.freeze(
  new Set([
    "texto", "titulo", "linha", "lista", "item", "botao", "arquivo",
    // As formas de composição existem **para** ter filhos: são elas que fazem
    // um MOD montar um componente próprio em vez de pedir um ao SEELE.
    "caixa", "pilha", "grade", "rolagem", "formulario", "acoes", "aba",
    "distintivo", "link",
  ]),
);

/**
 * As formas que o renderer trata como caixa de composição e nada mais.
 *
 * Elas não guardam recurso, não escutam evento e não têm conteúdo próprio: o
 * que aparece nelas é o que o MOD declarou dentro. A distinção existe para
 * `criar` não precisar de um `case` vazio para cada uma.
 */
const FORMAS_DE_CAIXA = Object.freeze(
  new Set(["caixa", "pilha", "grade", "rolagem", "acoes", "espaco", "separador", "distintivo"]),
);

/**
 * Os papéis que um `arquivo` pode pedir, e o que cada um filtra no seletor.
 *
 * **O filtro é orientação, não fronteira.** A extensão é texto que quem
 * escolheu digitou; o papel de verdade sai dos bytes, do lado do Rust, e é ele
 * que decide se o arquivo é aceito. O que estas listas fazem é não mostrar
 * quarenta JSONs a quem foi pedir um retrato — que foi o que a auditoria de
 * 20/09/2026 observou no seletor de avatar do PERFIS.
 */
const PAPEIS_DE_ARQUIVO = Object.freeze({
  imagem: Object.freeze(["png", "jpg", "jpeg", "webp", "gif"]),
  som: Object.freeze(["wav", "mp3", "ogg", "flac", "m4a"]),
});

/**
 * Os papéis que este `arquivo` pede, dentro dos que a API conhece.
 *
 * Sem declaração, os dois: é o comportamento que os pacotes já publicados têm,
 * e mudá-lo silenciosamente faria um MOD da API 3 parar de conseguir escolher.
 */
function papeisDeArquivo(no) {
  const pedidos = Array.isArray(no?.tipos) ? no.tipos : [];
  const validos = pedidos
    .map((t) => String(t))
    .filter((t) => Object.hasOwn(PAPEIS_DE_ARQUIVO, t));
  return validos.length ? [...new Set(validos)] : [];
}

/** O teto de bytes que este `arquivo` declara, dentro do que o produto aceita. */
function tetoDeArquivo(no) {
  const n = Number(no?.limiteDeBytes);
  if (!Number.isFinite(n) || n <= 0) return 0;
  return Math.min(Math.round(n), LIMITES_DA_REGIAO.bytesDeMidia);
}

/** O texto de um rótulo declarado por `rotulo` ou por `dentro`, quando é texto. */
function textoDoRotulo(no) {
  if (typeof no?.rotulo === "string" && no.rotulo) return no.rotulo.slice(0, 120);
  if (typeof no?.dentro === "string") return no.dentro.slice(0, 120);
  return "";
}

/**
 * O que o produto precisa saber antes de abrir o seletor do sistema.
 *
 * Três coisas, e as três são de quem escolhe: **para quê** (o título do
 * diálogo deixa de ser «Escolha um arquivo para este MOD» e passa a dizer o que
 * o MOD vai fazer com ele), **de que tipo** (o filtro de extensões, que é
 * orientação — a prova continua sendo os bytes) e **até quanto** (recusar antes
 * de ler, e não depois de a memória já ter pago).
 */
function pedidoDeArquivo(no) {
  const papeis = papeisDeArquivo(no);
  return {
    finalidade: typeof no?.finalidade === "string" ? no.finalidade.slice(0, 200) : "",
    papeis,
    extensoes: papeis.flatMap((p) => [...PAPEIS_DE_ARQUIVO[p]]),
    limiteDeBytes: tetoDeArquivo(no),
  };
}

/**
 * Uma região montada, com os recursos dela.
 *
 * O dono é o par `(instancia, geracao)`, e não o `id` do MOD: um MOD pode ser
 * recarregado dentro da mesma sessão, e o que a região de antes segurava não
 * pertence à instância de agora.
 */
class RegiaoDeMod {
  /** O contador que dá escopo único a cada raiz montada nesta janela. */
  static serieDeEscopo = 0;

  /**
   * @param {string} id `autor/nome`.
   * @param {object} dono `{ instancia, geracao, podeFalar, falar, carregarMidia }`.
   * @param {Element} raiz O elemento onde esta região desenha.
   */
  constructor(id, dono, raiz, perfil = PERFIS_DE_RENDER.regiao) {
    this.id = id;
    this.dono = dono;
    this.raiz = raiz;
    /** Qual orçamento e quais formas valem aqui — ver `PERFIS_DE_RENDER`. */
    this.perfil = perfil;
    /**
     * O escopo desta raiz, para as classes do MOD alcançarem **só ela**.
     *
     * Um número por raiz, e não o `id` do MOD: duas superfícies do mesmo MOD
     * com a classe `cartao` precisam de folhas separadas, senão a página
     * reescreveria o diálogo ao trocar uma cor.
     */
    this.escopo = `mod-${(RegiaoDeMod.serieDeEscopo += 1)}`;
    if (raiz) raiz.dataset.escopoDeMod = this.escopo;
    /** A folha de classes desta raiz, montada sob demanda. */
    this.folha = null;
    this.textoDaFolha = "";
    /** O que cada elemento segura, para soltar quando ele sai. */
    this.recursos = new Map();
    /** Quantos de cada forma com teto estão de pé. */
    this.contagem = { campos: 0, telas: 0, midias: 0, midiasDeCartao: 0 };
    /** Bytes de mídia montados nesta região. */
    this.bytesDeMidia = 0;
    /** Bytes de mídia montados nos cartões, contados à parte. */
    this.bytesDeCartao = 0;
    /** A raiz do cartão de cada pessoa, por `id` em texto. */
    this.raizesDeCartao = new Map();
    /** Quantos nós a última montagem recusou, por teto. */
    this.recusados = 0;
    /** Onde cada figura ficou em cada tela, para o toque saber o que pegou. */
    this.acertoDaTela = new Map();
    /** Já está solta? Soltar duas vezes não pode soltar o que não é dela. */
    this.solta = false;
  }

  /**
   * Monta o que o MOD declarou, **reaproveitando o que já está lá**.
   *
   * É aqui que mora a atualização incremental. A versão anterior chamava
   * `replaceChildren`, e isso significa que **todo** nó saía do documento e
   * voltava: um campo em edição perdia o foco, o cursor, a seleção e o que
   * estava sendo composto por um IME a cada atualização que o MOD mandasse. Um
   * MOD que redesenha a cada tecla era, por construção, um MOD em que não se
   * consegue digitar.
   *
   * @param {*} conteudo O que o MOD declarou.
   * @returns {number} Quantos nós foram recusados por teto.
   */
  aplicar(conteudo) {
    if (this.solta) return 0;
    this.recusados = 0;
    const orcamento = { nos: this.perfil.nos };
    this.reconciliar(
      this.raiz,
      this.planejar(conteudo, 0, orcamento, this.perfil.formas),
      0,
      orcamento,
    );
    return this.recusados;
  }

  /**
   * As classes que esta raiz oferece aos nós dela.
   *
   * Compiladas uma vez por declaração e guardadas num `<style>` do produto,
   * preso à raiz. Recompilar a cada desenho faria o navegador reanalisar a
   * folha a cada tecla digitada num campo.
   */
  declararClasses(classes) {
    if (this.solta || !this.raiz) return 0;
    const conta = { recusados: 0 };
    const texto = folhaDeClassesDeMod(this.escopo, classes, conta);
    if (texto === this.textoDaFolha) return conta.recusados;
    this.textoDaFolha = texto;
    if (!texto) {
      this.folha?.remove();
      this.folha = null;
      return conta.recusados;
    }
    if (!this.folha) {
      this.folha = document.createElement("style");
      // Registrada como recurso: uma folha que sobrevive à saída pinta a
      // sessão seguinte com as cores da anterior.
      this.dono.instancia?.registrar(`${this.id}: a folha de ${this.escopo}`, () => {
        this.folha?.remove();
        this.folha = null;
      });
      this.raiz.prepend(this.folha);
    }
    this.folha.textContent = texto;
    return conta.recusados;
  }

  /**
   * O que o MOD declarou, virado numa lista de planos — **antes** de virar DOM.
   *
   * Planejar antes de montar é o que faz o teto ser conferido sem custo: um
   * plano é um objeto pequeno, e um nó é layout. Recusar na montagem faria a
   * árvore de dez mil nós ser paga antes de ser negada.
   */
  planejar(no, fundura, orcamento, permitidas = null) {
    const teto = this.perfil.fundura;
    if (fundura > teto || no === null || no === undefined) return [];
    if (Array.isArray(no)) {
      return no.flatMap((um) => this.planejar(um, fundura, orcamento, permitidas));
    }
    if (typeof no === "string") {
      if (orcamento.nos <= 0) { this.recusados += 1; return []; }
      orcamento.nos -= 1;
      return [{ texto: no.slice(0, LIMITES_DA_REGIAO.texto) }];
    }
    if (typeof no !== "object") return [];

    let forma = FORMAS_DA_REGIAO[no.forma] ? no.forma : "";
    // Uma forma que a API não conhece não vira `div` por conveniência: virar
    // seria a gramática crescer sem ninguém decidir.
    //
    // E num cartão, uma forma que a **região** conhece mas o cartão não é
    // recusada do mesmo jeito, e contada: o MOD precisa saber que pôs um botão
    // onde botão não entra, em vez de vê-lo sumir.
    if (forma && permitidas && !permitidas.has(forma)) forma = "";
    if (!forma) {
      if (permitidas) this.recusados += 1;
      return [];
    }
    if (orcamento.nos <= 0) { this.recusados += 1; return []; }
    orcamento.nos -= 1;

    // A chave é o que faz a atualização ser incremental. Sem uma, a posição
    // serve — e um MOD que não dá chave nenhuma continua funcionando, só perde
    // o reaproveitamento quando reordena.
    const chave = typeof no.chave === "string" && no.chave
      ? `k:${no.chave}`
      : `p:${forma}:${orcamento.nos}`;
    return [{
      forma,
      chave,
      no,
      // **De quem são os tetos deste nó.** Uma mídia de cartão não conta no
      // orçamento da região, e o contrário também não: os dois desenham em
      // telas diferentes, com números diferentes, e somá-los faria um retrato
      // na lista tirar um som da região sem explicação nenhuma.
      cartao: Boolean(permitidas),
      dentro: this.planejar(no.dentro, fundura + 1, orcamento, permitidas),
    }];
  }

  /**
   * Põe os planos no lugar, mexendo no mínimo.
   *
   * O laço anda pelos filhos que já existem em vez de refazê-los. Um nó que
   * continua no mesmo lugar **não é tocado**, e é nisso que o foco sobrevive:
   * tirar um elemento com foco do documento tira o foco dele, e devolvê-lo
   * depois não o devolve.
   */
  reconciliar(pai, planos, fundura, orcamento) {
    const porChave = new Map();
    for (const filho of Array.from(pai.children)) {
      const chave = filho.dataset?.chave;
      if (chave !== undefined && !porChave.has(chave)) porChave.set(chave, filho);
    }

    let anterior = null;
    for (const plano of planos) {
      let no;
      if (plano.texto !== undefined) {
        // Um nó de texto não tem chave nem foco: a posição basta, e reaproveitá-lo
        // é uma escrita em `data` em vez de um nó novo.
        const candidato = anterior ? anterior.nextSibling : pai.firstChild;
        if (candidato && candidato.nodeType === Node.TEXT_NODE) {
          if (candidato.data !== plano.texto) candidato.data = plano.texto;
          no = candidato;
        } else {
          no = document.createTextNode(plano.texto);
        }
      } else {
        const velho = porChave.get(plano.chave);
        if (velho && velho.dataset.forma === plano.forma) {
          porChave.delete(plano.chave);
          no = velho;
          this.atualizar(no, plano);
        } else {
          no = this.criar(plano);
          if (!no) { this.recusados += 1; continue; }
        }
        if (FORMAS_COM_FILHOS.has(plano.forma)) {
          this.reconciliar(no, plano.dentro, fundura + 1, orcamento);
        }
      }
      const atual = anterior ? anterior.nextSibling : pai.firstChild;
      // **Só move o que precisa mover.** `insertBefore` de um nó que já está
      // na posição ainda o remove e reinsere, e remover é o que tira o foco.
      if (atual !== no) pai.insertBefore(no, atual);
      anterior = no;
    }

    // O que sobrou depois do último plano sai, e solta o que segurava.
    for (;;) {
      const sobra = anterior ? anterior.nextSibling : pai.firstChild;
      if (!sobra) break;
      this.soltarSubarvore(sobra);
      sobra.remove();
    }
  }

  /** Monta um nó novo, ou nada quando ele não cabe num teto. */
  criar(plano) {
    // Uma mídia de cartão tem contador e teto próprios: ver `plano.cartao`.
    const classe = plano.cartao && (plano.forma === "midia" || plano.forma === "retrato")
      ? "midiasDeCartao"
      : FORMAS_COM_TETO[plano.forma];
    const limite = classe === "midiasDeCartao"
      ? LIMITES_DO_CARTAO.midias
      : this.perfil[classe];
    if (classe && this.contagem[classe] >= limite) return null;

    // Por `elemento`, que é o construtor desta casa — `createElement` mais
    // `textContent` — e é também por onde o guarda de CSS enxerga as classes
    // que um script aplica. Montar por fora dele põe classe sem regra na tela
    // e nada avisa: foi assim que uma lista inteira perdeu o padding dela.
    const elem = elemento(FORMAS_DA_REGIAO[plano.forma], "regiao-de-mod-parte");
    elem.dataset.chave = plano.chave;
    elem.dataset.forma = plano.forma;
    // A chave que o MOD deu, guardada à parte da chave de reconciliação: é ela
    // que `valoresDoFormulario` lê para montar o envio.
    if (typeof plano.no.chave === "string" && plano.no.chave) {
      elem.dataset.chaveDoMod = plano.no.chave.slice(0, 120);
    }
    if (classe) this.contagem[classe] += 1;

    switch (plano.forma) {
      case "campo": this.montarCampo(elem, plano); break;
      case "escolha": this.montarEscolha(elem, plano); break;
      case "botao": this.montarBotao(elem, plano); break;
      case "arquivo": this.montarArquivo(elem, plano); break;
      case "tela": this.montarTela(elem, plano); break;
      case "midia": this.montarMidia(elem, plano); break;
      // ---- API 4 ----
      case "formulario": this.montarFormulario(elem, plano); break;
      case "abas": this.montarAbas(elem, plano); break;
      case "textoLongo": this.montarTextoLongo(elem, plano); break;
      case "numero": this.montarNumero(elem, plano); break;
      case "deslizante": this.montarDeslizante(elem, plano); break;
      case "marca":
      case "interruptor": this.montarMarca(elem, plano); break;
      case "cor": this.montarCor(elem, plano); break;
      case "retrato": this.montarRetrato(elem, plano); break;
      case "link": this.montarLink(elem, plano); break;
      default: break;
    }
    this.atualizar(elem, plano);
    return elem;
  }

  /** Reescreve o que mudou num nó que já existe. */
  atualizar(elem, plano) {
    // **Estilo e classe primeiro, e para toda forma.** Eles são a parte da API
    // 4 que não é uma forma nova: um `texto` com `estilo` é a diferença entre
    // «o MOD escreve o nome» e «o MOD apresenta a identidade» — que foi o que
    // U27 disse faltar.
    //
    // O que o validador não reconhecer é contado como recusa, pela regra de
    // sempre: um MOD que escreveu `corDeFundo` em vez de `fundo` descobre onde
    // escreveu, em vez de ver a cor não aparecer e não saber por quê.
    if (plano.no.estilo) {
      const conta = { recusados: 0 };
      aplicarEstiloDeMod(elem, plano.no.estilo, conta);
      this.recusados += conta.recusados;
    } else if (elem.dataset.estiloDeMod) {
      // O estilo saiu da declaração: o que ele escreveu sai da tela junto.
      aplicarEstiloDeMod(elem, null);
    }
    this.vestirClasses(elem, plano.no.classe);
    const rotuloAcessivel = typeof plano.no.nomeAcessivel === "string"
      ? plano.no.nomeAcessivel.slice(0, 200)
      : "";
    if (rotuloAcessivel) elem.setAttribute("aria-label", rotuloAcessivel);

    switch (plano.forma) {
      case "campo": this.atualizarCampo(elem, plano); break;
      case "escolha": this.atualizarEscolha(elem, plano); break;
      case "botao": this.atualizarBotao(elem, plano); break;
      case "arquivo": this.atualizarArquivo(elem, plano); break;
      case "tela": this.pintarTela(elem, plano); break;
      case "midia": this.atualizarMidia(elem, plano); break;
      // ---- API 4 ----
      case "formulario": this.atualizarFormulario(elem, plano); break;
      case "abas": this.atualizarAbas(elem, plano); break;
      case "aba": this.atualizarAba(elem, plano); break;
      case "textoLongo": this.atualizarTextoLongo(elem, plano); break;
      case "numero": this.atualizarNumero(elem, plano); break;
      case "deslizante": this.atualizarDeslizante(elem, plano); break;
      case "marca":
      case "interruptor": this.atualizarMarca(elem, plano); break;
      case "cor": this.atualizarCor(elem, plano); break;
      case "retrato": this.atualizarRetrato(elem, plano); break;
      case "link": this.atualizarLink(elem, plano); break;
      case "grade": break;
      default: break;
    }
  }

  /**
   * As classes do MOD num nó, sem tocar nas do produto.
   *
   * `classList` e não `className`: a classe do produto — `regiao-de-mod-parte`
   * — é a régua de espaçamento e o que o guarda de CSS enxerga. Trocar a lista
   * inteira a apagaria, e foi assim que uma lista perdeu o padding dela uma vez.
   */
  vestirClasses(elem, declarado) {
    const querem = classesDeMod(declarado);
    const tinham = elem.dataset.classesDeMod ? elem.dataset.classesDeMod.split(" ") : [];
    for (const antiga of tinham) {
      if (antiga && !querem.includes(antiga)) elem.classList.remove(antiga);
    }
    for (const nova of querem) elem.classList.add(nova);
    if (querem.length) elem.dataset.classesDeMod = querem.join(" ");
    else delete elem.dataset.classesDeMod;
  }

  /** O botão, com a variante que o MOD pediu e o estado de ação em curso. */
  atualizarBotao(elem, plano) {
    elem.disabled = plano.no.desligado === true || plano.no.emProgresso === true;
    const variantes = ["primaria", "secundaria", "perigo", "discreta"];
    const variante = variantes.includes(plano.no.variante) ? plano.no.variante : "secundaria";
    if (elem.dataset.variante !== variante) elem.dataset.variante = variante;
    // **«Em progresso» é um estado da ação, e não um botão desligado.** §7 do
    // plano: «Botão em progresso impede duplicata conforme regra da operação;
    // mantém feedback visual imediato.» Sem o atributo, um clique duplo vira
    // duas campanhas e a pessoa não vê nada acontecer entre os dois.
    if (plano.no.emProgresso === true) {
      elem.dataset.progresso = "sim";
      elem.setAttribute("aria-busy", "true");
    } else {
      delete elem.dataset.progresso;
      elem.removeAttribute("aria-busy");
    }
    // Por que está indisponível, quando o MOD souber dizer.
    const porque = typeof plano.no.porqueIndisponivel === "string"
      ? plano.no.porqueIndisponivel.slice(0, 200)
      : "";
    if (porque && elem.disabled) elem.title = porque;
    else if (elem.title && elem.dataset.forma === "botao") elem.removeAttribute("title");
  }

  // ------------------------------------------------------------ campo

  /**
   * Um campo editável: rótulo, caixa, e o que a pessoa escreve indo ao MOD.
   *
   * O `<label>` é o nó com chave, e a `<input>` mora dentro: o rótulo precisa
   * estar amarrado à caixa para quem navega por leitor de tela, e amarrar por
   * `for`/`id` exigiria um `id` global escolhido por um MOD.
   */
  montarCampo(elem, plano) {
    const rotulo = elemento("span", "regiao-de-mod-rotulo");
    const caixa = elemento("input", "regiao-de-mod-caixa");
    caixa.type = "text";
    caixa.maxLength = LIMITES_DA_REGIAO.valorDoCampo;
    elem.append(rotulo, caixa);
    const aoDigitar = () => {
      this.dono.falar({
        nome: "campo",
        chave: plano.no.chave ?? "",
        valor: caixa.value.slice(0, LIMITES_DA_REGIAO.valorDoCampo),
      });
    };
    caixa.addEventListener("input", aoDigitar);
    this.guardar(elem, `campo ${plano.chave}`, () => {
      caixa.removeEventListener("input", aoDigitar);
      this.contagem.campos -= 1;
    });
  }

  /**
   * O rótulo e o valor, e **o valor só quando a caixa não está sendo usada**.
   *
   * Reescrever `value` numa caixa com foco move o cursor para o fim e apaga a
   * seleção, mesmo quando o texto é igual ao que já estava. Um MOD que ecoa o
   * que recebe — que é o que um MOD que grava no servidor faz — tornaria a
   * digitação impossível a partir da segunda letra.
   */
  atualizarCampo(elem, plano) {
    const rotulo = elem.querySelector(".regiao-de-mod-rotulo");
    const caixa = elem.querySelector(".regiao-de-mod-caixa");
    if (!rotulo || !caixa) return;
    const texto = typeof plano.no.rotulo === "string" ? plano.no.rotulo : "";
    if (rotulo.textContent !== texto) rotulo.textContent = texto;
    if (document.activeElement === caixa) return;
    const valor = typeof plano.no.valor === "string"
      ? plano.no.valor.slice(0, LIMITES_DA_REGIAO.valorDoCampo)
      : "";
    if (caixa.value !== valor) caixa.value = valor;
  }

  // ------------------------------------------------------------ escolha

  /**
   * Uma escolha entre opções que o MOD declara.
   *
   * **Não é um campo com validação.** Um campo aceita qualquer texto e o MOD
   * precisa conferir; aqui o produto só deixa sair um dos valores declarados, e
   * é isso que faz uma paleta, uma fonte ou uma densidade não precisarem de
   * conferência do outro lado.
   */
  montarEscolha(elem, plano) {
    const rotulo = elemento("span", "regiao-de-mod-rotulo");
    const caixa = elemento("select", "regiao-de-mod-escolha");
    elem.append(rotulo, caixa);
    const aoTrocar = () => {
      this.dono.falar({
        nome: "escolha",
        chave: plano.no.chave ?? "",
        valor: caixa.value,
      });
    };
    caixa.addEventListener("change", aoTrocar);
    this.guardar(elem, `escolha ${plano.chave}`, () => {
      caixa.removeEventListener("change", aoTrocar);
      this.contagem.campos -= 1;
    });
  }

  /**
   * As opções e a escolhida — e a escolhida **só quando a caixa não tem foco**.
   *
   * Pela mesma razão do campo: reescrever uma lista aberta a fecha, e quem
   * estava percorrendo as opções com o teclado perde onde estava.
   */
  atualizarEscolha(elem, plano) {
    const rotulo = elem.querySelector(".regiao-de-mod-rotulo");
    const caixa = elem.querySelector(".regiao-de-mod-escolha");
    if (!rotulo || !caixa) return;
    const texto = typeof plano.no.rotulo === "string" ? plano.no.rotulo : "";
    if (rotulo.textContent !== texto) rotulo.textContent = texto;
    if (document.activeElement === caixa) return;

    const opcoes = (Array.isArray(plano.no.opcoes) ? plano.no.opcoes : [])
      .slice(0, LIMITES_DA_REGIAO.opcoes)
      .map((o) => ({
        valor: String(o?.valor ?? "").slice(0, 64),
        dentro: String(o?.dentro ?? o?.valor ?? "").slice(0, 120),
      }));
    // Refeitas só quando mudaram: refazer a lista a cada desenho fecharia a
    // caixa aberta de quem está escolhendo, mesmo sem foco no elemento.
    const assinatura = opcoes.map((o) => `${o.valor}\u0000${o.dentro}`).join("\u0001");
    if (caixa.dataset.opcoes !== assinatura) {
      caixa.dataset.opcoes = assinatura;
      const nos = opcoes.map((o) => {
        const item = elemento("option", "regiao-de-mod-opcao", o.dentro);
        item.value = o.valor;
        return item;
      });
      caixa.replaceChildren(...nos);
    }
    const valor = String(plano.no.valor ?? "");
    if (caixa.value !== valor) caixa.value = valor;
  }

  // ------------------------------------------------------------ botão

  montarBotao(elem, plano) {
    elem.type = "button";
    const aoApertar = () => {
      this.dono.falar({ nome: "botao", chave: plano.no.chave ?? "" });
    };
    elem.addEventListener("click", aoApertar);
    this.guardar(elem, `botao ${plano.chave}`, () => {
      elem.removeEventListener("click", aoApertar);
    });
  }

  // ------------------------------------------------------------ arquivo

  /**
   * **Escolher um arquivo é um ato de quem usa, e o produto é quem o media.**
   *
   * Um botão, e o seletor do sistema atrás dele. O MOD não lista pasta, não
   * abre caminho e não recebe nome de arquivo: ele recebe um **identificador**
   * e o que o produto provou sobre os bytes — o tipo, saído deles, e o tamanho.
   *
   * É a diferença entre mediar e entregar o disco. Sem o botão, um MOD que
   * precisa de um retrato não tem caminho nenhum; com acesso ao disco, ele tem
   * o caminho errado.
   */
  montarArquivo(elem, plano) {
    elem.type = "button";
    let escolhendo = false;
    const aoApertar = () => {
      // Um seletor de cada vez: dois diálogos abertos pelo mesmo botão é uma
      // janela que quem usa não sabe qual fechar.
      if (escolhendo) return;
      escolhendo = true;
      elem.disabled = true;
      elem.dataset.escolhendo = "sim";
      Promise.resolve(this.dono.escolherArquivo(pedidoDeArquivo(plano.no)))
        .then((escolhido) => {
          // **Cancelar é uma resposta.** Quem fecha o seletor sem escolher faz
          // o MOD receber `null`, e não um silêncio que o deixa esperando.
          this.dono.falar({
            nome: "arquivo",
            chave: plano.no.chave ?? "",
            arquivo: escolhido ?? null,
            // «Não escolhi» e «não deu» são respostas diferentes, e antes elas
            // chegavam iguais: as duas como `arquivo: null`. Um MOD que quisesse
            // dizer «o formato não serve» não tinha como distinguir do silêncio
            // de quem fechou o seletor — e a auditoria pediu cancelamento
            // «neutro e silencioso quando apropriado», que só é possível quando
            // o MOD sabe que foi cancelamento.
            resultado: escolhido ? "escolhido" : "cancelado",
          });
        })
        .catch((falha) => {
          this.dono.falar({
            nome: "arquivo",
            chave: plano.no.chave ?? "",
            arquivo: null,
            resultado: "falhou",
            porque: String(falha?.message ?? falha),
          });
        })
        .finally(() => {
          escolhendo = false;
          delete elem.dataset.escolhendo;
          elem.disabled = plano.no.desligado === true;
        });
    };
    elem.addEventListener("click", aoApertar);
    this.guardar(elem, `arquivo ${plano.chave}`, () => {
      elem.removeEventListener("click", aoApertar);
      // O seletor aberto não some daqui — ele é do sistema —, mas a escolha
      // que voltar encontra a região solta e não fala com ninguém.
      escolhendo = false;
    });
  }

  /**
   * O rótulo, a finalidade e o estado — e **por que isto faltava**.
   *
   * `arquivo` não estava em `FORMAS_COM_FILHOS`, e nem `montarArquivo` nem
   * `atualizarArquivo` escreviam texto: um `<button>` vazio, sem nome
   * acessível, saía na tela. O PERFIS declarava `ENVIAR RETRATO` e
   * `ENVIAR FAIXA` e a auditoria de 20/09/2026 encontrou dois botões em
   * branco, um deles abrindo um seletor genérico que não dizia para quê.
   *
   * Não era um pacote que esqueceu o texto: era o renderer que não tinha
   * caminho nenhum para escrevê-lo. Agora `dentro` reconcilia como no `botao`,
   * e `finalidade` vira o nome acessível completo — que é o que um leitor de
   * tela lê antes de o diálogo do sistema abrir.
   */
  atualizarArquivo(elem, plano) {
    elem.disabled = plano.no.desligado === true;
    const finalidade = typeof plano.no.finalidade === "string"
      ? plano.no.finalidade.slice(0, 200)
      : "";
    const rotulo = textoDoRotulo(plano.no);
    // O nome acessível é o rótulo mais a finalidade quando ela existe: «ENVIAR
    // RETRATO» sozinho não diz que o arquivo vai virar o avatar do perfil.
    const nome = finalidade ? `${rotulo} — ${finalidade}` : rotulo;
    if (nome) {
      if (elem.getAttribute("aria-label") !== nome) elem.setAttribute("aria-label", nome);
      if (elem.getAttribute("title") !== nome) elem.setAttribute("title", nome);
    } else {
      elem.removeAttribute("aria-label");
      elem.removeAttribute("title");
    }
    // Os tipos e o teto ficam legíveis na própria tela, e não só no diálogo do
    // sistema: quem decide se tem o arquivo decide **antes** de abrir o seletor.
    const papeis = papeisDeArquivo(plano.no);
    const teto = tetoDeArquivo(plano.no);
    const exigencia = [
      papeis.length ? papeis.map((p) => (p === "som" ? "som" : "imagem")).join(" ou ") : "",
      teto ? `até ${Math.round(teto / 1024)} KB` : "",
    ].filter(Boolean).join(" · ");
    if (exigencia) elem.dataset.exigencia = exigencia;
    else delete elem.dataset.exigencia;
  }

  // ------------------------------------------- os controles da API 4
  //
  // Todos seguem a mesma forma dos da API 3, e por um motivo que não é
  // simetria: o que fazia um campo ser utilizável era **não reescrever o
  // valor enquanto a caixa tem foco**. Um MOD que ecoa o que recebe — que é o
  // que um MOD que grava no servidor faz — tornaria a digitação impossível a
  // partir da segunda letra. A regra vale igual para textarea, número,
  // deslizante e cor.

  /**
   * Uma caixa de texto de várias linhas.
   *
   * U20 pediu esta forma pelo nome: «"Sobre mim" é input de uma linha» e
   * «biografia multilinha e prévia». Não era uma escolha do PERFIS — a API 3
   * não tinha como declarar outra coisa.
   */
  montarTextoLongo(elem, plano) {
    const rotulo = elemento("span", "regiao-de-mod-rotulo");
    const caixa = elemento("textarea", "regiao-de-mod-area");
    caixa.rows = 4;
    caixa.maxLength = LIMITES_DA_REGIAO.valorDoCampo * 8;
    elem.append(rotulo, caixa);
    const aoDigitar = () => {
      this.dono.falar({
        nome: "campo",
        chave: plano.no.chave ?? "",
        valor: caixa.value.slice(0, caixa.maxLength),
      });
    };
    caixa.addEventListener("input", aoDigitar);
    this.guardar(elem, `textoLongo ${plano.chave}`, () => {
      caixa.removeEventListener("input", aoDigitar);
      this.contagem.campos -= 1;
    });
  }

  atualizarTextoLongo(elem, plano) {
    const rotulo = elem.querySelector(".regiao-de-mod-rotulo");
    const caixa = elem.querySelector(".regiao-de-mod-area");
    if (!rotulo || !caixa) return;
    const texto = typeof plano.no.rotulo === "string" ? plano.no.rotulo : "";
    if (rotulo.textContent !== texto) rotulo.textContent = texto;
    const linhas = Math.min(Math.max(Number(plano.no.linhas) || 4, 2), 24);
    if (caixa.rows !== linhas) caixa.rows = linhas;
    const sugestao = typeof plano.no.sugestao === "string"
      ? plano.no.sugestao.slice(0, 120)
      : "";
    if (caixa.placeholder !== sugestao) caixa.placeholder = sugestao;
    this.marcarValidade(elem, caixa, plano);
    if (document.activeElement === caixa) return;
    const valor = typeof plano.no.valor === "string"
      ? plano.no.valor.slice(0, caixa.maxLength)
      : "";
    if (caixa.value !== valor) caixa.value = valor;
  }

  /** Um número, com os limites que o MOD declarou e o produto confere. */
  montarNumero(elem, plano) {
    const rotulo = elemento("span", "regiao-de-mod-rotulo");
    const caixa = elemento("input", "regiao-de-mod-caixa");
    caixa.type = "number";
    elem.append(rotulo, caixa);
    const aoDigitar = () => {
      // **Vazio é vazio, e não zero.** Um campo apagado no meio da edição não
      // pode virar `0` a caminho do MOD: o MOD gravaria zero.
      const bruto = caixa.value;
      this.dono.falar({
        nome: "numero",
        chave: plano.no.chave ?? "",
        valor: bruto === "" ? null : Number(bruto),
        texto: bruto,
      });
    };
    caixa.addEventListener("input", aoDigitar);
    this.guardar(elem, `numero ${plano.chave}`, () => {
      caixa.removeEventListener("input", aoDigitar);
      this.contagem.campos -= 1;
    });
  }

  atualizarNumero(elem, plano) {
    const rotulo = elem.querySelector(".regiao-de-mod-rotulo");
    const caixa = elem.querySelector(".regiao-de-mod-caixa");
    if (!rotulo || !caixa) return;
    const texto = typeof plano.no.rotulo === "string" ? plano.no.rotulo : "";
    if (rotulo.textContent !== texto) rotulo.textContent = texto;
    for (const [atributo, chave] of [["min", "minimo"], ["max", "maximo"], ["step", "passo"]]) {
      const n = Number(plano.no[chave]);
      if (Number.isFinite(n)) caixa.setAttribute(atributo, String(n));
      else caixa.removeAttribute(atributo);
    }
    this.marcarValidade(elem, caixa, plano);
    if (document.activeElement === caixa) return;
    const valor = plano.no.valor === null || plano.no.valor === undefined
      ? ""
      : String(plano.no.valor);
    if (caixa.value !== valor) caixa.value = valor;
  }

  /** Um deslizante, com o valor legível ao lado — que é o que o torna usável. */
  montarDeslizante(elem, plano) {
    const rotulo = elemento("span", "regiao-de-mod-rotulo");
    const caixa = elemento("input", "regiao-de-mod-deslizante");
    caixa.type = "range";
    const lido = elemento("output", "regiao-de-mod-valor");
    elem.append(rotulo, caixa, lido);
    const aoMexer = () => {
      lido.textContent = caixa.value;
      this.dono.falar({
        nome: "numero",
        chave: plano.no.chave ?? "",
        valor: Number(caixa.value),
      });
    };
    caixa.addEventListener("input", aoMexer);
    this.guardar(elem, `deslizante ${plano.chave}`, () => {
      caixa.removeEventListener("input", aoMexer);
      this.contagem.campos -= 1;
    });
  }

  atualizarDeslizante(elem, plano) {
    const rotulo = elem.querySelector(".regiao-de-mod-rotulo");
    const caixa = elem.querySelector(".regiao-de-mod-deslizante");
    const lido = elem.querySelector(".regiao-de-mod-valor");
    if (!rotulo || !caixa) return;
    const texto = typeof plano.no.rotulo === "string" ? plano.no.rotulo : "";
    if (rotulo.textContent !== texto) rotulo.textContent = texto;
    caixa.min = String(Number(plano.no.minimo) || 0);
    caixa.max = String(Number(plano.no.maximo) || 100);
    caixa.step = String(Number(plano.no.passo) || 1);
    if (document.activeElement !== caixa) {
      const valor = String(Number(plano.no.valor) || 0);
      if (caixa.value !== valor) caixa.value = valor;
    }
    if (lido && lido.textContent !== caixa.value) lido.textContent = caixa.value;
  }

  /** Uma marca — caixa de seleção ou interruptor, na mesma mecânica. */
  montarMarca(elem, plano) {
    const caixa = elemento("input", "regiao-de-mod-marca");
    caixa.type = "checkbox";
    const rotulo = elemento("span", "regiao-de-mod-rotulo");
    elem.append(caixa, rotulo);
    const aoTrocar = () => {
      this.dono.falar({
        nome: "marca",
        chave: plano.no.chave ?? "",
        valor: caixa.checked,
      });
    };
    caixa.addEventListener("change", aoTrocar);
    this.guardar(elem, `marca ${plano.chave}`, () => {
      caixa.removeEventListener("change", aoTrocar);
      this.contagem.campos -= 1;
    });
  }

  atualizarMarca(elem, plano) {
    const caixa = elem.querySelector(".regiao-de-mod-marca");
    const rotulo = elem.querySelector(".regiao-de-mod-rotulo");
    if (!caixa || !rotulo) return;
    const texto = typeof plano.no.rotulo === "string" ? plano.no.rotulo : "";
    if (rotulo.textContent !== texto) rotulo.textContent = texto;
    caixa.disabled = plano.no.desligado === true;
    // `switch` é papel, e não etiqueta: um interruptor e uma caixa de seleção
    // são o mesmo controle com leituras diferentes para quem usa leitor de tela.
    if (plano.forma === "interruptor") caixa.setAttribute("role", "switch");
    else caixa.removeAttribute("role");
    if (document.activeElement === caixa) return;
    const ligado = plano.no.valor === true;
    if (caixa.checked !== ligado) caixa.checked = ligado;
  }

  /**
   * Uma cor: seletor visual **e** hexadecimal, lado a lado.
   *
   * U23 descreveu o problema exato: «ESTILO usa inputs de hexadecimal em vez
   * de seletor e amostra; a cor muda após gravar, sem prévia reversível». O
   * seletor resolve a escolha, o campo hexadecimal resolve colar um valor
   * conhecido, e a amostra resolve ver o resultado antes de gravar.
   */
  montarCor(elem, plano) {
    const rotulo = elemento("span", "regiao-de-mod-rotulo");
    const seletor = elemento("input", "regiao-de-mod-cor");
    seletor.type = "color";
    const hexa = elemento("input", "regiao-de-mod-caixa regiao-de-mod-hexa");
    hexa.type = "text";
    hexa.maxLength = 9;
    hexa.spellcheck = false;
    hexa.setAttribute("aria-label", "Valor hexadecimal");
    elem.append(rotulo, seletor, hexa);

    const dizer = (valor) => {
      this.dono.falar({ nome: "cor", chave: plano.no.chave ?? "", valor });
    };
    const aoEscolher = () => {
      hexa.value = seletor.value;
      dizer(seletor.value);
    };
    const aoDigitar = () => {
      const escrito = hexa.value.trim();
      // Um hexadecimal incompleto — `#f2` no meio da digitação — não vira
      // pedido: ele viraria uma cor recusada a cada tecla.
      if (/^#[0-9a-f]{6}$/i.test(escrito)) {
        seletor.value = escrito;
        elem.dataset.invalido = "nao";
        dizer(escrito);
      } else {
        elem.dataset.invalido = escrito ? "sim" : "nao";
      }
    };
    seletor.addEventListener("input", aoEscolher);
    hexa.addEventListener("input", aoDigitar);
    this.guardar(elem, `cor ${plano.chave}`, () => {
      seletor.removeEventListener("input", aoEscolher);
      hexa.removeEventListener("input", aoDigitar);
      this.contagem.campos -= 1;
    });
  }

  atualizarCor(elem, plano) {
    const rotulo = elem.querySelector(".regiao-de-mod-rotulo");
    const seletor = elem.querySelector(".regiao-de-mod-cor");
    const hexa = elem.querySelector(".regiao-de-mod-hexa");
    if (!rotulo || !seletor || !hexa) return;
    const texto = typeof plano.no.rotulo === "string" ? plano.no.rotulo : "";
    if (rotulo.textContent !== texto) rotulo.textContent = texto;
    const valor = typeof plano.no.valor === "string" && /^#[0-9a-f]{6}$/i.test(plano.no.valor)
      ? plano.no.valor
      : "#000000";
    if (document.activeElement !== seletor && seletor.value !== valor) seletor.value = valor;
    if (document.activeElement !== hexa && hexa.value !== valor) hexa.value = valor;
  }

  /**
   * O retrato de uma pessoa: imagem quando há, inicial quando não há.
   *
   * Forma própria e não `midia` porque a composição é diferente: um avatar tem
   * recorte, proporção fixa e um estado de ausência que não é um erro. Um
   * `<figure>` com `<img>` quebrada não é a mesma coisa que uma inicial num
   * círculo, e U27 pediu a segunda.
   */
  montarRetrato(elem, plano) {
    const marca = elemento("span", "regiao-de-mod-retrato-inicial");
    elem.append(marca);
    const estado = { elemento: null, cancelado: false, bytes: 0 };
    this.guardar(elem, `retrato ${plano.chave}`, () => {
      estado.cancelado = true;
      const img = estado.elemento;
      if (img) { img.removeAttribute("src"); img.remove(); }
      const bolso = plano.cartao ? "bytesDeCartao" : "bytesDeMidia";
      this[bolso] -= estado.bytes;
      this.contagem[plano.cartao ? "midiasDeCartao" : "midias"] -= 1;
    });
    elem.dataset.estadoDoRetrato = "inicial";
    const doServidor = plano.no.doServidor;
    const caminho = typeof plano.no.fonte === "string" ? plano.no.fonte : "";
    const vindo = doServidor
      ? this.dono.carregarMidiaDoServidor(
          Number(doServidor.canal) || 0,
          doServidor.pedido ?? {},
          typeof doServidor.campo === "string" && doServidor.campo ? doServidor.campo : "bytes",
        )
      : caminho
        ? this.dono.carregarMidia(caminho)
        : null;
    if (!vindo) return;
    vindo.then((midia) => {
      if (estado.cancelado || this.solta || !this.dono.podeFalar()) return;
      if (midia.papel !== "imagem") return;
      const bolso = plano.cartao
        ? { conta: "bytesDeCartao", teto: LIMITES_DO_CARTAO.bytesDeMidia }
        : { conta: "bytesDeMidia", teto: this.perfil.bytesDeMidia };
      if (this[bolso.conta] + midia.bytes > bolso.teto) {
        elem.dataset.estadoDoRetrato = "cheia";
        return;
      }
      const img = elemento("img", "regiao-de-mod-retrato-imagem");
      img.alt = typeof plano.no.descricao === "string" ? plano.no.descricao : "";
      img.src = midia.uri;
      estado.elemento = img;
      estado.bytes = midia.bytes;
      this[bolso.conta] += midia.bytes;
      elem.append(img);
      elem.dataset.estadoDoRetrato = "imagem";
    }).catch(() => {
      if (estado.cancelado || this.solta) return;
      // Falhar num retrato **não** é um erro que a pessoa precise ler: a
      // inicial continua lá e responde a mesma pergunta.
      elem.dataset.estadoDoRetrato = "inicial";
    });
  }

  atualizarRetrato(elem, plano) {
    const marca = elem.querySelector(".regiao-de-mod-retrato-inicial");
    if (marca) {
      const inicial = String(plano.no.inicial ?? "?").trim().charAt(0).toUpperCase() || "?";
      if (marca.textContent !== inicial) marca.textContent = inicial;
    }
    const formas = ["circulo", "quadrado", "arredondado"];
    const forma = formas.includes(plano.no.formato) ? plano.no.formato : "circulo";
    if (elem.dataset.formato !== forma) elem.dataset.formato = forma;
    const img = elem.querySelector(".regiao-de-mod-retrato-imagem");
    if (img) {
      const alt = typeof plano.no.descricao === "string" ? plano.no.descricao : "";
      if (img.alt !== alt) img.alt = alt;
    }
  }

  /**
   * Um link **mediado**: o MOD nomeia um endereço, e o produto o abre fora.
   *
   * `<button>` e não `<a href>`: um `href` numa janela de aplicativo é uma
   * navegação de verdade, e uma navegação leva a página da conversa embora. O
   * que o produto faz é o que já faz com link de mensagem — abrir no navegador
   * do sistema, depois de a pessoa ter clicado.
   */
  montarLink(elem, plano) {
    elem.type = "button";
    const aoApertar = () => {
      const url = typeof plano.no.endereco === "string" ? plano.no.endereco : "";
      this.dono.abrirEndereco(url, plano.no.chave ?? "");
    };
    elem.addEventListener("click", aoApertar);
    this.guardar(elem, `link ${plano.chave}`, () => {
      elem.removeEventListener("click", aoApertar);
    });
  }

  atualizarLink(elem, plano) {
    const url = typeof plano.no.endereco === "string" ? plano.no.endereco : "";
    // **O endereço fica visível.** Um botão que diz «clique aqui» e leva a
    // outro lugar é a forma mais antiga de enganar alguém, e o MOD não é quem
    // decide se quem lê merece saber para onde vai.
    if (elem.title !== url) elem.title = url;
    elem.disabled = plano.no.desligado === true || !url;
  }

  // ------------------------------------------------------- formulário

  /**
   * Um formulário: valores locais, envio, e o `Enter` que não recarrega nada.
   *
   * `<form>` de verdade e não uma `div` com botões, por três coisas que só o
   * elemento dá: `Enter` num campo envia, leitores de tela anunciam o grupo, e
   * o navegador associa rótulos e erros. O `submit` é interceptado — numa
   * WebView, deixá-lo seguir recarrega a aplicação inteira.
   */
  montarFormulario(elem, plano) {
    const aoEnviar = (evento) => {
      evento.preventDefault();
      this.dono.falar({
        nome: "submeter",
        chave: plano.no.chave ?? "",
        valores: this.valoresDoFormulario(elem),
      });
    };
    elem.addEventListener("submit", aoEnviar);
    this.guardar(elem, `formulario ${plano.chave}`, () => {
      elem.removeEventListener("submit", aoEnviar);
    });
  }

  atualizarFormulario(elem, plano) {
    const erro = typeof plano.no.erro === "string" ? plano.no.erro.slice(0, 400) : "";
    if (elem.dataset.erro !== erro) {
      if (erro) elem.dataset.erro = erro;
      else delete elem.dataset.erro;
    }
  }

  /**
   * O que está escrito no formulário **agora**, por chave.
   *
   * Lido do DOM e não do último estado que o MOD mandou, e a diferença importa:
   * o §5 do plano diz que «a entrada ecoa localmente no host» — o que a pessoa
   * digitou está aqui antes de o MOD ter recebido a última tecla.
   */
  valoresDoFormulario(raiz) {
    const valores = {};
    const campos = raiz.querySelectorAll(
      "[data-forma=\"campo\"],[data-forma=\"escolha\"],[data-forma=\"textoLongo\"],"
      + "[data-forma=\"numero\"],[data-forma=\"deslizante\"],[data-forma=\"marca\"],"
      + "[data-forma=\"interruptor\"],[data-forma=\"cor\"]",
    );
    for (const campo of campos) {
      const chave = campo.dataset.chaveDoMod;
      if (!chave) continue;
      const caixa = campo.querySelector("input,select,textarea");
      if (!caixa) continue;
      if (caixa.type === "checkbox") valores[chave] = caixa.checked;
      else if (caixa.type === "number" || caixa.type === "range") {
        valores[chave] = caixa.value === "" ? null : Number(caixa.value);
      } else valores[chave] = caixa.value;
    }
    return valores;
  }

  /** Marca um controle como inválido, para a folha de classes poder pintá-lo. */
  marcarValidade(elem, caixa, plano) {
    const erro = typeof plano.no.erro === "string" ? plano.no.erro.slice(0, 200) : "";
    if (erro) {
      elem.dataset.invalido = "sim";
      caixa.setAttribute("aria-invalid", "true");
      if (elem.dataset.mensagemDeErro !== erro) elem.dataset.mensagemDeErro = erro;
    } else {
      delete elem.dataset.invalido;
      delete elem.dataset.mensagemDeErro;
      caixa.removeAttribute("aria-invalid");
    }
  }

  // ------------------------------------------------------------- abas

  /**
   * Abas com teclado, e **ativação manual**.
   *
   * O APG da WAI-ARIA descreve as duas: automática (a seta já troca o painel)
   * e manual (a seta move o foco, Enter troca). A escolha aqui é manual porque
   * o painel de um MOD pode custar caro para montar — uma ficha inteira, um
   * tabuleiro — e percorrer cinco abas com a seta montaria os cinco.
   *
   * Só o painel ativo é montado, pela mesma razão. O §5 do plano é explícito:
   * «Abas não podem montar todos os conteúdos pesados antecipadamente só para
   * navegar por setas.»
   */
  montarAbas(elem, plano) {
    const tiras = elemento("div", "regiao-de-mod-tiras");
    tiras.setAttribute("role", "tablist");
    const palco = elemento("div", "regiao-de-mod-palco");
    elem.append(tiras, palco);
    const aoTeclar = (evento) => {
      const botoes = Array.from(tiras.querySelectorAll("[role=\"tab\"]"));
      const atual = botoes.indexOf(document.activeElement);
      if (atual < 0) return;
      const passo = evento.key === "ArrowRight" ? 1
        : evento.key === "ArrowLeft" ? -1
          : evento.key === "Home" ? -atual
            : evento.key === "End" ? botoes.length - 1 - atual
              : 0;
      if (!passo) return;
      evento.preventDefault();
      const destino = (atual + passo + botoes.length) % botoes.length;
      botoes[destino]?.focus();
    };
    tiras.addEventListener("keydown", aoTeclar);
    this.guardar(elem, `abas ${plano.chave}`, () => {
      tiras.removeEventListener("keydown", aoTeclar);
    });
  }

  atualizarAbas(elem, plano) {
    const tiras = elem.querySelector(".regiao-de-mod-tiras");
    const palco = elem.querySelector(".regiao-de-mod-palco");
    if (!tiras || !palco) return;
    const abas = plano.dentro.filter((p) => p.forma === "aba");
    const escolhida = String(plano.no.valor ?? abas[0]?.no?.chave ?? "");

    // As tiras são refeitas só quando a lista mudou: refazê-las a cada desenho
    // tiraria o foco de quem está percorrendo com as setas.
    const assinatura = abas
      .map((a) => `${a.no.chave}\u0000${a.no.rotulo ?? ""}\u0000${a.no.desligado === true}`)
      .join("\u0001");
    if (tiras.dataset.abas !== assinatura) {
      tiras.dataset.abas = assinatura;
      const botoes = abas.map((aba) => {
        const botao = elemento("button", "regiao-de-mod-tira", String(aba.no.rotulo ?? aba.no.chave ?? ""));
        botao.type = "button";
        botao.setAttribute("role", "tab");
        botao.dataset.aba = String(aba.no.chave ?? "");
        botao.disabled = aba.no.desligado === true;
        return botao;
      });
      tiras.replaceChildren(...botoes);
      // Um ouvinte na tira, e não um por botão: refazer a lista jogaria fora
      // um ouvinte por aba a cada desenho.
      if (!tiras.dataset.ouvindo) {
        tiras.dataset.ouvindo = "sim";
        const aoApertar = (evento) => {
          const alvo = evento.target.closest?.("[data-aba]");
          if (!alvo || alvo.disabled) return;
          this.dono.falar({ nome: "aba", chave: plano.no.chave ?? "", valor: alvo.dataset.aba });
        };
        tiras.addEventListener("click", aoApertar);
        this.guardar(tiras, `tiras ${plano.chave}`, () => {
          tiras.removeEventListener("click", aoApertar);
        });
      }
    }
    for (const botao of tiras.children) {
      const ativa = botao.dataset.aba === escolhida;
      botao.setAttribute("aria-selected", ativa ? "true" : "false");
      // Uma parada de tabulação para o grupo inteiro, como o APG pede: Tab
      // entra nas abas e sai delas, e as setas percorrem por dentro.
      botao.tabIndex = ativa ? 0 : -1;
    }

    // **Só o painel escolhido é montado.** Ver a razão em `montarAbas`.
    const ativa = abas.find((a) => String(a.no.chave ?? "") === escolhida) ?? abas[0];
    this.reconciliar(palco, ativa ? [ativa] : [], 0, { nos: this.perfil.nos });
  }

  atualizarAba(elem, plano) {
    elem.setAttribute("role", "tabpanel");
    elem.tabIndex = 0;
    const rotulo = typeof plano.no.rotulo === "string" ? plano.no.rotulo : "";
    if (rotulo) elem.setAttribute("aria-label", rotulo);
  }

  // ------------------------------------------------------------ tela

  /**
   * Uma tela de desenho: o arraste vira evento, e o traço vira pintura.
   *
   * **O MOD não desenha aqui; ele declara traços e recebe pontos.** É a mesma
   * regra da árvore: o que atravessa é dado, e quem pinta é o produto. Um
   * `CanvasRenderingContext2D` na mão do MOD seria a página de volta por outro
   * caminho — `drawImage` de um elemento desta janela lê pixels dela.
   */
  montarTela(elem, plano) {
    let arrastando = false;
    let pendente = null;
    let quadro = 0;
    // Qual figura foi pega ao descer o dedo. Ela viaja nas três fases: quem
    // arrasta uma peça precisa saber qual peça está arrastando enquanto o dedo
    // já saiu de cima dela.
    let pego = null;

    const ponto = (evento) => {
      const caixa = elem.getBoundingClientRect();
      // Nas coordenadas da tela declarada, e não nas da janela: o MOD não sabe
      // onde a região dele está, e não precisa saber.
      return {
        x: Math.round(((evento.clientX - caixa.left) / (caixa.width || 1)) * elem.width),
        y: Math.round(((evento.clientY - caixa.top) / (caixa.height || 1)) * elem.height),
      };
    };

    /** A figura mais em cima sob este ponto, ou nada. */
    const sob = ({ x, y }) => {
      for (const area of this.acertoDaTela.get(elem) ?? []) {
        if (
          typeof area.chave === "string" &&
          x >= area.x &&
          x <= area.x + area.largura &&
          y >= area.y &&
          y <= area.y + area.altura
        ) {
          return area.chave;
        }
      }
      return null;
    };

    const mandar = (fase, evento) => {
      const onde = ponto(evento);
      this.dono.falar({
        nome: "traco",
        chave: plano.no.chave ?? "",
        fase,
        ...onde,
        // `null` quando o toque caiu no vazio, e não ausente: «não peguei
        // nada» é uma resposta, e um campo que some é uma pergunta sem ela.
        alvo: pego,
      });
    };

    // **O movimento é agregado por quadro.** Um arraste produz centenas de
    // eventos por segundo, e cada um deles é uma mensagem na fila do executor.
    // Sem isto, arrastar é a forma mais fácil de saturar a fila — e o MOD
    // receberia um histórico que ele não consegue consumir na velocidade em
    // que ele chega.
    const escoar = () => {
      quadro = 0;
      if (!pendente || !arrastando) return;
      const evento = pendente;
      pendente = null;
      mandar("moveu", evento);
    };
    const aoDescer = (evento) => {
      arrastando = true;
      elem.setPointerCapture?.(evento.pointerId);
      // Decidido **aqui**, e não a cada movimento: o que se pega é o que estava
      // debaixo do dedo quando ele desceu. Recalcular no meio do arraste faria
      // a peça trocar de identidade ao passar por cima de outra.
      pego = sob(ponto(evento));
      mandar("comecou", evento);
    };
    const aoMover = (evento) => {
      if (!arrastando) return;
      pendente = evento;
      if (!quadro) quadro = requestAnimationFrame(escoar);
    };
    const aoSubir = (evento) => {
      if (!arrastando) return;
      arrastando = false;
      pendente = null;
      mandar("terminou", evento);
      pego = null;
    };
    elem.addEventListener("pointerdown", aoDescer);
    elem.addEventListener("pointermove", aoMover);
    elem.addEventListener("pointerup", aoSubir);
    elem.addEventListener("pointercancel", aoSubir);
    this.guardar(elem, `tela ${plano.chave}`, () => {
      elem.removeEventListener("pointerdown", aoDescer);
      elem.removeEventListener("pointermove", aoMover);
      elem.removeEventListener("pointerup", aoSubir);
      elem.removeEventListener("pointercancel", aoSubir);
      // **O quadro pendente também é recurso.** Um `requestAnimationFrame` que
      // sobrevive à saída roda uma vez depois de a sessão ter acabado, e o que
      // ele faz é falar com um executor que não existe mais.
      if (quadro) cancelAnimationFrame(quadro);
      quadro = 0;
      arrastando = false;
      pendente = null;
      pego = null;
      // A lista de acerto sai com a tela: ela descreve pixels que não existem
      // mais, e guardá-la manteria a região viva pelo próprio mapa.
      this.acertoDaTela.delete(elem);
      this.contagem.telas -= 1;
    });
  }

  /** Pinta os traços que o MOD declarou, e só eles. */
  pintarTela(elem, plano) {
    const lado = (valor, padrao) => {
      const n = Number(valor);
      if (!Number.isFinite(n) || n <= 0) return padrao;
      return Math.min(Math.round(n), LIMITES_DA_REGIAO.ladoDaTela);
    };
    const largura = lado(plano.no.largura, 240);
    const altura = lado(plano.no.altura, 160);
    // Mexer em `width`/`height` limpa a tela, então só quando mudou de verdade.
    if (elem.width !== largura) elem.width = largura;
    if (elem.height !== altura) elem.height = altura;

    const pincel = elem.getContext?.("2d");
    if (!pincel) return;
    pincel.clearRect(0, 0, elem.width, elem.height);
    const estilo = getComputedStyle(elem);
    pincel.strokeStyle = estilo.getPropertyValue("color") || "#ffffff";
    pincel.lineWidth = 2;
    pincel.lineCap = "round";
    pincel.lineJoin = "round";
    // **As figuras primeiro, os traços por cima.** Um tabuleiro é figura e o
    // que se rabisca nele é traço; pintar na outra ordem esconderia o rabisco
    // atrás da peça que veio depois.
    this.pintarFiguras(pincel, elem, plano);

    const tracos = Array.isArray(plano.no.tracos) ? plano.no.tracos : [];
    for (const traco of tracos.slice(0, LIMITES_DA_REGIAO.tracos)) {
      const pontos = Array.isArray(traco) ? traco.slice(0, LIMITES_DA_REGIAO.pontos) : [];
      if (pontos.length < 2) continue;
      pincel.beginPath();
      pontos.forEach((ponto, i) => {
        const x = Number(ponto?.x);
        const y = Number(ponto?.y);
        if (!Number.isFinite(x) || !Number.isFinite(y)) return;
        if (i === 0) pincel.moveTo(x, y);
        else pincel.lineTo(x, y);
      });
      pincel.stroke();
    }
    if (tracos.length > LIMITES_DA_REGIAO.tracos) this.recusados += 1;
  }

  /**
   * As figuras que o MOD declarou, pintadas e **guardadas para o acerto**.
   *
   * É o que separa uma tela de rabisco de um tabuleiro. Um traço é o gesto de
   * quem desenha; uma figura é uma coisa que está lá — uma peça, uma parede,
   * uma sala — e que pode ser **pega**. O MOD declara a figura com uma chave, o
   * produto guarda onde ela ficou, e o evento de arraste diz qual foi pega.
   *
   * Sem isto, arrastar uma peça só dava ao MOD um par de coordenadas, e cabia a
   * ele descobrir o que havia ali — refazendo o acerto que o produto acabou de
   * fazer para pintar, e refazendo-o pior, porque ele não sabe a ordem em que
   * as figuras ficaram na tela.
   *
   * A gramática é pequena pela mesma razão da outra: cada tipo novo é uma
   * decisão de API em vez de um MOD descobrir que consegue.
   */
  pintarFiguras(pincel, elem, plano) {
    const declaradas = Array.isArray(plano.no.figuras) ? plano.no.figuras : [];
    const figuras = declaradas.slice(0, LIMITES_DA_REGIAO.figuras);
    if (declaradas.length > figuras.length) this.recusados += 1;
    // A lista de acerto é refeita a cada pintura: ela **é** o que está na tela,
    // e uma lista velha faria o arraste pegar uma peça que já saiu.
    const acerto = [];
    const num = (valor, padrao = 0) => {
      const n = Number(valor);
      return Number.isFinite(n) ? n : padrao;
    };
    for (const figura of figuras) {
      if (!figura || typeof figura !== "object") continue;
      const x = num(figura.x);
      const y = num(figura.y);
      const largura = Math.max(0, num(figura.largura));
      const altura = Math.max(0, num(figura.altura));
      const cor = COR_DA_FIGURA.test(figura.cor ?? "") ? figura.cor : null;
      pincel.save();
      if (cor) {
        pincel.fillStyle = cor;
        pincel.strokeStyle = cor;
      }
      switch (figura.tipo) {
        case "retangulo":
          if (figura.preenchida === false) pincel.strokeRect(x, y, largura, altura);
          else pincel.fillRect(x, y, largura, altura);
          acerto.push({ chave: figura.chave, x, y, largura, altura });
          break;
        case "circulo": {
          const raio = Math.max(0, num(figura.raio));
          pincel.beginPath();
          pincel.arc(x, y, raio, 0, Math.PI * 2);
          if (figura.preenchida === false) pincel.stroke();
          else pincel.fill();
          acerto.push({ chave: figura.chave, x: x - raio, y: y - raio, largura: raio * 2, altura: raio * 2 });
          break;
        }
        case "texto": {
          const texto = String(figura.dentro ?? "").slice(0, 120);
          if (!texto) break;
          const corpo = Math.min(Math.max(num(figura.corpo, 12), 6), 64);
          pincel.font = `${corpo}px ${getComputedStyle(elem).fontFamily || "monospace"}`;
          pincel.textBaseline = "top";
          pincel.fillText(texto, x, y);
          acerto.push({
            chave: figura.chave,
            x,
            y,
            largura: pincel.measureText(texto).width,
            altura: corpo,
          });
          break;
        }
        case "linha": {
          pincel.beginPath();
          pincel.moveTo(x, y);
          pincel.lineTo(num(figura.ate_x, x), num(figura.ate_y, y));
          pincel.stroke();
          // Uma linha não é pega: ela é parede, régua, grade. Dar-lhe área de
          // acerto faria uma grade roubar o toque de toda peça em cima dela.
          break;
        }
        default:
          // Um tipo que a API não conhece não vira retângulo por conveniência.
          this.recusados += 1;
          break;
      }
      pincel.restore();
    }
    // **A última declarada está por cima**, e é ela que o toque encontra
    // primeiro: o acerto percorre ao contrário da pintura.
    this.acertoDaTela.set(elem, acerto.reverse());
  }

  // ------------------------------------------------------------ mídia

  /**
   * Mídia **gerenciada**: o produto monta, toca, para e solta.
   *
   * O MOD nomeia um arquivo que o manifesto dele declarou, e o produto o lê, o
   * reconhece pelos bytes e o monta. O MOD nunca tem o elemento na mão.
   *
   * Três coisas que só existem por ser gerenciada:
   *
   * - **o tipo vem dos bytes**, e o elemento sai do tipo — um MOD não escolhe
   *   o decodificador em que seus bytes caem;
   * - **a conta é por região**, em número e em bytes, e é ela que impede um MOD
   *   de montar mídia até a memória acabar;
   * - **o carregamento tem dono**. Sair durante o carregamento é o caso que a
   *   diretriz manda exercitar, e a resposta que chega depois da saída não
   *   monta nada: ela encontra a região já solta.
   */
  montarMidia(elem, plano) {
    // De qual bolso esta mídia sai. Decidido **aqui**, e guardado na closure:
    // decidi-lo depois do `await` exigiria um campo de instância, e um campo
    // de instância estaria valendo a mídia errada quando duas carregam juntas.
    const bolso = plano.cartao
      ? { conta: "bytesDeCartao", teto: LIMITES_DO_CARTAO.bytesDeMidia, quantas: "midiasDeCartao" }
      : { conta: "bytesDeMidia", teto: LIMITES_DA_REGIAO.bytesDeMidia, quantas: "midias" };
    const estado = { elemento: null, cancelado: false, bytes: 0, ouvintes: [] };
    this.guardar(elem, `midia ${plano.chave}`, () => {
      estado.cancelado = true;
      const tocador = estado.elemento;
      // **Os ouvintes saem por nome**, e não por o nó ser descartado. Um nó
      // solto leva os ouvintes dele embora, mas só quando o coletor passa — e
      // até lá cada um deles segura esta região, que segura a instância.
      for (const [nome, fn] of estado.ouvintes) tocador?.removeEventListener(nome, fn);
      estado.ouvintes.length = 0;
      if (tocador) {
        // Pausar **e** tirar a fonte: um `<audio>` removido do documento com
        // `src` continua com os bytes decodificados presos até o coletor
        // passar, e um que ainda não começou pode começar depois.
        tocador.pause?.();
        tocador.removeAttribute("src");
        tocador.load?.();
      }
      this[bolso.conta] -= estado.bytes;
      this.contagem[bolso.quantas] -= 1;
    });
    elem.dataset.estado = "carregando";
    // **Duas origens, e só duas.** Do pacote, por um arquivo que o manifesto
    // declara; ou do servidor deste MOD, por uma operação dele. Não há uma
    // terceira, e a que faltaria — um endereço qualquer — é justamente a que
    // faria a janela de quem conversa buscar bytes na rede de um estranho.
    const doServidor = plano.no.doServidor;
    const caminho = typeof plano.no.fonte === "string" ? plano.no.fonte : "";
    const vindo = doServidor
      ? this.dono.carregarMidiaDoServidor(
          Number(doServidor.canal) || 0,
          doServidor.pedido ?? {},
          // Qual campo da resposta traz o base64. O nome é do MOD, não do
          // produto: ditar um faria todo servidor existente renomear o dele.
          typeof doServidor.campo === "string" && doServidor.campo
            ? doServidor.campo
            : "bytes",
        )
      : caminho
        ? this.dono.carregarMidia(caminho)
        : null;
    if (!vindo) { elem.dataset.estado = "sem-fonte"; return; }

    vindo.then((midia) => {
      // **As três perguntas, depois do `await`.** A região pode ter sido
      // solta, o nó pode ter saído da árvore, e a sessão pode ter acabado —
      // e montar mídia em qualquer um dos três casos é tocar som de uma
      // sessão que já não existe.
      if (estado.cancelado || this.solta || !this.dono.podeFalar()) return;
      if (this[bolso.conta] + midia.bytes > bolso.teto) {
        elem.dataset.estado = "cheia";
        this.dono.falar({ nome: "midia", chave: plano.no.chave ?? "", estado: "recusada" });
        return;
      }
      // A etiqueta sai do papel que o Rust devolveu, e o papel saiu dos bytes:
      // um MOD não escolhe o decodificador em que os bytes dele caem.
      const tocador = elemento(midia.papel === "som" ? "audio" : "img", "regiao-de-mod-tocador");
      if (midia.papel === "som") tocador.controls = true;
      else tocador.alt = typeof plano.no.descricao === "string" ? plano.no.descricao : "";
      tocador.src = midia.uri;
      estado.elemento = tocador;
      estado.bytes = midia.bytes;
      this[bolso.conta] += midia.bytes;
      elem.dataset.estado = "pronta";
      elem.append(tocador);
      for (const [nome, aviso] of [["play", "tocando"], ["pause", "pausada"], ["ended", "terminou"], ["error", "falhou"]]) {
        const ouvinte = () => {
          this.dono.falar({ nome: "midia", chave: plano.no.chave ?? "", estado: aviso });
        };
        tocador.addEventListener(nome, ouvinte);
        estado.ouvintes.push([nome, ouvinte]);
      }
      this.atualizarMidia(elem, plano);
    }).catch((falha) => {
      if (estado.cancelado || this.solta) return;
      elem.dataset.estado = "falhou";
      this.dono.falar({
        nome: "midia",
        chave: plano.no.chave ?? "",
        estado: "falhou",
        porque: String(falha?.message ?? falha),
      });
    });
  }

  /** O MOD pede «tocando» e o produto obedece, **se** puder. */
  atualizarMidia(elem, plano) {
    const tocador = elem.querySelector(".regiao-de-mod-tocador");
    if (!tocador || typeof tocador.play !== "function") return;
    if (plano.no.tocando === true) {
      // `play()` devolve promessa e ela **rejeita** quando o navegador não
      // deixa tocar sem gesto. Ignorá-la faria o MOD achar que está tocando;
      // o evento de erro é o que diz a verdade.
      tocador.play().catch(() => {
        this.dono.falar({ nome: "midia", chave: plano.no.chave ?? "", estado: "recusada" });
      });
    } else if (!tocador.paused) {
      tocador.pause();
    }
  }

  // -------------------------------------------------------- os cartões

  /**
   * Os cartões que este MOD declara para a lista de pessoas do produto.
   *
   * # Por que isto não é o MOD desenhando na janela
   *
   * O ADR 0049 tirou o MOD da janela, e nada aqui o traz de volta. O que chega
   * é **a mesma declaração da região** — as mesmas formas, o mesmo
   * `planejar`, o mesmo `reconciliar`, o mesmo `elemento` — montada pelo mesmo
   * renderer, com `createElement` e `textContent` e nunca com HTML de texto.
   * O MOD não alcança nó nenhum, não escolhe onde o cartão entra na linha, e
   * não recebe evento de dentro dele.
   *
   * O que muda em relação à região é o que ele **pode** declarar (ver
   * `FORMAS_DO_CARTAO`) e quanto (ver `LIMITES_DO_CARTAO`). Um cartão é
   * desenhado ao lado do nome de uma pessoa, numa lista que o produto usa para
   * dizer quem está falando: um teto de região ali seria a lateral inteira.
   *
   * # O descarte
   *
   * A raiz de cada cartão é guardada aqui, e não só no documento. Quando um
   * MOD para de declarar uma pessoa, a raiz dela sai e solta o que segurava —
   * e quando a região inteira sai, `soltar` percorre estas raízes também.
   * Sem isso, o retrato de um MOD que não está mais de pé continuaria na lista
   * até alguém trocar de servidor.
   *
   * @param {object} cartoes `{ [id da pessoa]: declaração }`.
   * @returns {number} quantos nós foram recusados por teto ou por forma.
   */
  declararCartoes(cartoes) {
    if (this.solta) return 0;
    this.recusados = 0;
    const pedidos = Object.entries(cartoes ?? {});
    if (pedidos.length > LIMITES_DO_CARTAO.cartoes) {
      throw new Error(
        `um MOD dá cartão a até ${LIMITES_DO_CARTAO.cartoes} pessoas, e vieram ${pedidos.length}`,
      );
    }

    const vivos = new Set();
    for (const [quem, declaracao] of pedidos) {
      const pessoa = String(quem);
      // Um cartão sem nada dentro não é um cartão: seria uma moldura vazia ao
      // lado de um nome, que é o produto anunciando uma ausência que ninguém
      // pediu para anunciar.
      const orcamento = { nos: LIMITES_DO_CARTAO.nos };
      const planos = this.planejar(declaracao, 0, orcamento, PERFIS_DE_RENDER.cartao.formas);
      if (planos.length === 0) continue;
      vivos.add(pessoa);

      let raiz = this.raizesDeCartao.get(pessoa);
      if (!raiz) {
        raiz = elemento("div", "pessoa-cartao");
        raiz.dataset.mod = this.id;
        this.raizesDeCartao.set(pessoa, raiz);
      }
      this.reconciliar(raiz, planos, 0, orcamento);
    }

    for (const [pessoa, raiz] of Array.from(this.raizesDeCartao)) {
      if (vivos.has(pessoa)) continue;
      this.soltarCartao(pessoa, raiz);
    }
    return this.recusados;
  }

  /** Tira o cartão de uma pessoa e solta o que ele segurava. */
  soltarCartao(pessoa, raiz) {
    this.raizesDeCartao.delete(pessoa);
    this.soltarSubarvore(raiz);
    for (const filho of Array.from(raiz.children)) this.soltarSubarvore(filho);
    raiz.remove();
  }

  /** A raiz do cartão desta pessoa, ou nada quando este MOD não deu um. */
  cartaoDe(pessoa) {
    if (this.solta) return null;
    return this.raizesDeCartao.get(String(pessoa)) ?? null;
  }

  // ------------------------------------------------------- os recursos

  /**
   * Amarra um descarte a um elemento, e o registra na instância.
   *
   * **Os dois, e não um.** No elemento para sair quando o nó sai da árvore; na
   * instância para sair quando a sessão acaba, que é quando não há mais árvore
   * para percorrer.
   */
  guardar(elem, porque, descartar) {
    let soltou = false;
    const umaVez = () => {
      if (soltou) return;
      soltou = true;
      descartar();
    };
    this.recursos.set(elem, { porque, descartar: umaVez });
    this.dono.instancia?.registrar(`${this.id}: ${porque}`, umaVez);
  }

  /** Solta o que esta subárvore segurava, de baixo para cima. */
  soltarSubarvore(no) {
    if (!no || no.nodeType !== Node.ELEMENT_NODE) return;
    for (const filho of Array.from(no.children)) this.soltarSubarvore(filho);
    const recurso = this.recursos.get(no);
    if (!recurso) return;
    this.recursos.delete(no);
    try {
      recurso.descartar();
    } catch (falha) {
      console.warn(`MOD ${this.id}: ${recurso.porque} não saiu`, falha);
    }
  }

  /**
   * Tira a região inteira, e solta tudo.
   *
   * Idempotente de propósito: a saída solta as regiões e o encerramento da
   * instância solta os recursos dela, e os dois caminhos chegam aqui.
   */
  soltar() {
    if (this.solta) return;
    this.solta = true;
    for (const filho of Array.from(this.raiz.children)) this.soltarSubarvore(filho);
    // **E os cartões**, que moram na lista do produto e não sob esta raiz. Um
    // retrato de um MOD que saiu continuaria ao lado do nome de alguém até a
    // próxima troca de servidor — e ninguém teria como tirá-lo.
    for (const [pessoa, raiz] of Array.from(this.raizesDeCartao)) {
      this.soltarCartao(pessoa, raiz);
    }
    // O que não estava mais na árvore — porque o nó saiu antes — já foi solto
    // pelo mesmo caminho; o que restar aqui é o que a árvore não alcançava.
    for (const [, recurso] of this.recursos) {
      try {
        recurso.descartar();
      } catch (falha) {
        console.warn(`MOD ${this.id}: ${recurso.porque} não saiu`, falha);
      }
    }
    this.recursos.clear();
    this.raiz.remove();
  }
}
