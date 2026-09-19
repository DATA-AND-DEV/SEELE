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

/** As formas que a API conhece, e a etiqueta que cada uma vira. */
const FORMAS_DA_REGIAO = Object.freeze({
  texto: "p",
  titulo: "h3",
  linha: "div",
  lista: "ul",
  item: "li",
  campo: "label",
  escolha: "label",
  botao: "button",
  tela: "canvas",
  midia: "figure",
});

/** A cor de uma figura, na mesma forma que o tema aceita. */
const COR_DA_FIGURA = /^#[0-9a-f]{6}$/i;

/** As que criam recurso, e por isso passam por contador próprio. */
const FORMAS_COM_TETO = Object.freeze({
  campo: "campos",
  escolha: "campos",
  tela: "telas",
  midia: "midias",
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
  new Set(["texto", "titulo", "linha", "lista", "item", "botao"]),
);

/**
 * Uma região montada, com os recursos dela.
 *
 * O dono é o par `(instancia, geracao)`, e não o `id` do MOD: um MOD pode ser
 * recarregado dentro da mesma sessão, e o que a região de antes segurava não
 * pertence à instância de agora.
 */
class RegiaoDeMod {
  /**
   * @param {string} id `autor/nome`.
   * @param {object} dono `{ instancia, geracao, podeFalar, falar, carregarMidia }`.
   * @param {Element} raiz O elemento onde esta região desenha.
   */
  constructor(id, dono, raiz) {
    this.id = id;
    this.dono = dono;
    this.raiz = raiz;
    /** O que cada elemento segura, para soltar quando ele sai. */
    this.recursos = new Map();
    /** Quantos de cada forma com teto estão de pé. */
    this.contagem = { campos: 0, telas: 0, midias: 0 };
    /** Bytes de mídia montados nesta região. */
    this.bytesDeMidia = 0;
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
    const orcamento = { nos: LIMITES_DA_REGIAO.nos };
    this.reconciliar(this.raiz, this.planejar(conteudo, 0, orcamento), 0, orcamento);
    return this.recusados;
  }

  /**
   * O que o MOD declarou, virado numa lista de planos — **antes** de virar DOM.
   *
   * Planejar antes de montar é o que faz o teto ser conferido sem custo: um
   * plano é um objeto pequeno, e um nó é layout. Recusar na montagem faria a
   * árvore de dez mil nós ser paga antes de ser negada.
   */
  planejar(no, fundura, orcamento) {
    if (fundura > LIMITES_DA_REGIAO.fundura || no === null || no === undefined) return [];
    if (Array.isArray(no)) return no.flatMap((um) => this.planejar(um, fundura, orcamento));
    if (typeof no === "string") {
      if (orcamento.nos <= 0) { this.recusados += 1; return []; }
      orcamento.nos -= 1;
      return [{ texto: no.slice(0, LIMITES_DA_REGIAO.texto) }];
    }
    if (typeof no !== "object") return [];

    const forma = FORMAS_DA_REGIAO[no.forma] ? no.forma : "";
    // Uma forma que a API não conhece não vira `div` por conveniência: virar
    // seria a gramática crescer sem ninguém decidir.
    if (!forma) return [];
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
      dentro: this.planejar(no.dentro, fundura + 1, orcamento),
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
    const classe = FORMAS_COM_TETO[plano.forma];
    if (classe && this.contagem[classe] >= LIMITES_DA_REGIAO[classe]) return null;

    // Por `elemento`, que é o construtor desta casa — `createElement` mais
    // `textContent` — e é também por onde o guarda de CSS enxerga as classes
    // que um script aplica. Montar por fora dele põe classe sem regra na tela
    // e nada avisa: foi assim que uma lista inteira perdeu o padding dela.
    const elem = elemento(FORMAS_DA_REGIAO[plano.forma], "regiao-de-mod-parte");
    elem.dataset.chave = plano.chave;
    elem.dataset.forma = plano.forma;
    if (classe) this.contagem[classe] += 1;

    switch (plano.forma) {
      case "campo": this.montarCampo(elem, plano); break;
      case "escolha": this.montarEscolha(elem, plano); break;
      case "botao": this.montarBotao(elem, plano); break;
      case "tela": this.montarTela(elem, plano); break;
      case "midia": this.montarMidia(elem, plano); break;
      default: break;
    }
    this.atualizar(elem, plano);
    return elem;
  }

  /** Reescreve o que mudou num nó que já existe. */
  atualizar(elem, plano) {
    switch (plano.forma) {
      case "campo": this.atualizarCampo(elem, plano); break;
      case "escolha": this.atualizarEscolha(elem, plano); break;
      case "botao": elem.disabled = plano.no.desligado === true; break;
      case "tela": this.pintarTela(elem, plano); break;
      case "midia": this.atualizarMidia(elem, plano); break;
      default: break;
    }
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
      this.bytesDeMidia -= estado.bytes;
      this.contagem.midias -= 1;
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
      if (this.bytesDeMidia + midia.bytes > LIMITES_DA_REGIAO.bytesDeMidia) {
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
      this.bytesDeMidia += midia.bytes;
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
