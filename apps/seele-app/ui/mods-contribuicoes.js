// Onde um MOD entra na interface do SEELE, e o que ele pode fazer lá.
//
// ADR 0052, §6 do plano da API de criação de interfaces. A auditoria de
// 20/09/2026 formulou o requisito assim: «Este é um requisito de primeira
// classe. Não basta criar conteúdo ao lado de uma linha que continua
// intocável.»
//
// # O que este arquivo é, e o que ele recusa ser
//
// Ele é um **registro de pontos semânticos**. Um MOD diz «no cartão desta
// pessoa, use esta apresentação»; o produto decide onde o cartão fica, quem o
// monta, o que acontece ao clicar e o que sobra quando o MOD sai.
//
// Ele **não** é acesso ao documento. Não há aqui, e não deve haver:
// `querySelector`, nome de classe interna, `innerHTML`, folha global. A razão
// não é desconfiança — é que um seletor é um contrato que ninguém escreveu:
// no dia em que a conversa renomear `.roster-linha`, todo MOD publicado quebra
// junto, e nenhum dos dois lados combinou isso.
//
// # As três garantias que o registro dá
//
// **Identidade.** A apresentação troca o que se **desenha**; o comando
// continua ligado ao ID real. Um MOD pode escrever outro nome no cartão de
// alguém — é para isso que o PERFIS existe —, e o menu de moderação daquela
// linha continua agindo sobre a pessoa certa. §6: «o host liga as ações ao ID
// real, nunca ao texto desenhado».
//
// **Determinismo.** Duas substituições do mesmo ponto não se resolvem por
// «quem respondeu por último». Elas se resolvem por prioridade declarada e,
// empatadas, pela ordem de instalação — que é estável e visível.
//
// **Recuperação.** Ao revogar, a apresentação nativa é **recalculada do estado
// atual**, e não restaurada de um `innerHTML` guardado. A pessoa pode ter
// mudado de sala enquanto o MOD estava aberto, e devolver a linha antiga seria
// devolver uma informação falsa.

/**
 * Os pontos de extensão, e o que cada um aceita.
 *
 * `modos` é o que se pode fazer ali. A distinção não é burocrática:
 *
 * - `adicionar` convive com o nativo e com outros MODs, em grupo previsível;
 * - `substituir` toma o lugar do nativo, e por isso é exclusivo: dois MODs
 *   pedindo o mesmo ponto precisam de uma decisão, e ela é tomada aqui;
 * - `decorar` altera atributos de apresentação de um nó do produto sem trocar
 *   o conteúdo dele — é o que permite pôr uma cor num canal sem assumir o
 *   desenho do item inteiro.
 *
 * `perfil` diz com que orçamento a declaração é montada. Um cartão custa por
 * pessoa; uma página custa uma vez.
 */
const PONTOS_DE_CONTRIBUICAO = Object.freeze({
  "pessoa.identidade": { modos: ["substituir", "decorar"], perfil: "cartao", porAlvo: true },
  "pessoa.cartao": { modos: ["substituir", "adicionar"], perfil: "cartao", porAlvo: true },
  "pessoa.detalhes": { modos: ["adicionar"], perfil: "superficie", porAlvo: true },
  "pessoa.acoes": { modos: ["adicionar"], perfil: "cartao", porAlvo: true },
  "canal.item": { modos: ["decorar", "adicionar"], perfil: "cartao", porAlvo: true },
  "canal.cabecalho": { modos: ["adicionar"], perfil: "cartao", porAlvo: true },
  "compositor.ferramentas": { modos: ["adicionar"], perfil: "cartao", porAlvo: false },
  "sala.acoes": { modos: ["adicionar"], perfil: "cartao", porAlvo: true },
  "servidor.navegacao": { modos: ["adicionar"], perfil: null, porAlvo: false },
  "servidor.aparencia": { modos: ["substituir"], perfil: null, porAlvo: false },
});

/** Quantas contribuições um MOD registra ao mesmo tempo. */
const TETO_DE_CONTRIBUICOES = 128;

/**
 * O registro vivo de contribuições desta janela.
 *
 * Chaveado por `(ponto, alvo)` porque é assim que a consulta acontece: a lista
 * de pessoas pergunta «quem contribui para `pessoa.cartao` da pessoa 12?»
 * enquanto monta a linha dela, uma vez por pessoa por desenho. Uma varredura
 * da lista inteira por linha seria O(pessoas × contribuições) a cada retrato.
 */
class RegistroDeContribuicoes {
  constructor() {
    /** `Map<ponto, Map<alvo, Array<contribuicao>>>`. `alvo` vazio é «todos». */
    this.porPonto = new Map();
    /** Tudo por `handle`, para revogar sem varrer. */
    this.porHandle = new Map();
    /** Quantas cada MOD tem de pé, para o teto. */
    this.porMod = new Map();
    this.serie = 0;
    /** Quem avisar quando algo muda. A sessão redesenha o que precisa. */
    this.ouvintes = new Set();
  }

  /**
   * Registra uma contribuição e devolve o `handle` que a revoga.
   *
   * @param {object} mod `{ id }`.
   * @param {object} instancia A instância do executor, para o recurso ter dono.
   * @param {object} pedido `{ ponto, modo, alvo, prioridade, conteudo, ... }`.
   * @returns {{ handle: string, recusados: number }}
   */
  registrar(mod, instancia, pedido) {
    const ponto = String(pedido?.ponto ?? "");
    const regra = Object.hasOwn(PONTOS_DE_CONTRIBUICAO, ponto)
      ? PONTOS_DE_CONTRIBUICAO[ponto]
      : null;
    // **Recusado e nomeado.** Um ponto que não existe não pode virar silêncio:
    // o MOD ficaria esperando para sempre uma contribuição que nunca aparece.
    if (!regra) {
      throw new Error(
        `a API de MODs não conhece o ponto «${ponto}»; os que existem são `
        + Object.keys(PONTOS_DE_CONTRIBUICAO).join(", "),
      );
    }
    const modo = String(pedido?.modo ?? "adicionar");
    if (!regra.modos.includes(modo)) {
      throw new Error(`«${ponto}» aceita ${regra.modos.join(" ou ")}, e veio «${modo}»`);
    }
    const quantas = this.porMod.get(mod.id) ?? 0;
    if (quantas >= TETO_DE_CONTRIBUICOES) {
      throw new Error(`um MOD mantém até ${TETO_DE_CONTRIBUICOES} contribuições de pé`);
    }

    const alvo = regra.porAlvo && pedido?.alvo !== undefined && pedido?.alvo !== null
      ? String(pedido.alvo)
      : "";
    const contribuicao = {
      handle: `c${(this.serie += 1)}`,
      mod: mod.id,
      instancia,
      geracao: instancia?.geracao ?? 0,
      ponto,
      modo,
      alvo,
      // A prioridade é do MOD, dentro de um intervalo. Ela ordena; ela não
      // autoriza: duas substituições continuam precisando de uma decisão, e é
      // `escolherSubstituicao` quem a toma.
      prioridade: Math.min(Math.max(Number(pedido?.prioridade) || 0, -100), 100),
      conteudo: pedido?.conteudo ?? null,
      classes: pedido?.classes ?? null,
      acaoPrincipal: typeof pedido?.acaoPrincipal === "string" ? pedido.acaoPrincipal : "",
      nomeAcessivel: typeof pedido?.nomeAcessivel === "string"
        ? pedido.nomeAcessivel.slice(0, 200)
        : "",
      rotulo: typeof pedido?.rotulo === "string" ? pedido.rotulo.slice(0, 120) : "",
      icone: typeof pedido?.icone === "string" ? pedido.icone.slice(0, 40) : "",
      ordem: this.serie,
    };

    const porAlvo = this.porPonto.get(ponto) ?? new Map();
    const lista = porAlvo.get(alvo) ?? [];
    lista.push(contribuicao);
    // Ordenada na inserção, e não na leitura: a leitura acontece por pessoa por
    // desenho, e ordenar ali seria ordenar vinte vezes por retrato.
    lista.sort((a, b) => b.prioridade - a.prioridade || a.ordem - b.ordem);
    porAlvo.set(alvo, lista);
    this.porPonto.set(ponto, porAlvo);
    this.porHandle.set(contribuicao.handle, contribuicao);
    this.porMod.set(mod.id, quantas + 1);

    // **Registrada como recurso na criação.** Uma contribuição que a saída não
    // encontra é uma faixa de perfil que sobrevive à troca de servidor.
    instancia?.registrar(`${mod.id}: contribuição ${ponto}`, () => {
      this.revogar(contribuicao.handle);
    });
    this.avisar();
    return { handle: contribuicao.handle };
  }

  /** Tira uma contribuição, e avisa quem desenha. Idempotente. */
  revogar(handle) {
    const contribuicao = this.porHandle.get(handle);
    if (!contribuicao) return false;
    this.porHandle.delete(handle);
    const porAlvo = this.porPonto.get(contribuicao.ponto);
    const lista = porAlvo?.get(contribuicao.alvo);
    if (lista) {
      const onde = lista.indexOf(contribuicao);
      if (onde >= 0) lista.splice(onde, 1);
      if (!lista.length) porAlvo.delete(contribuicao.alvo);
    }
    if (porAlvo && !porAlvo.size) this.porPonto.delete(contribuicao.ponto);
    const quantas = this.porMod.get(contribuicao.mod) ?? 0;
    if (quantas <= 1) this.porMod.delete(contribuicao.mod);
    else this.porMod.set(contribuicao.mod, quantas - 1);
    this.avisar();
    return true;
  }

  /** Tira tudo o que um MOD tem de pé. Usado ao descarregá-lo. */
  revogarDoMod(id) {
    for (const [handle, contribuicao] of Array.from(this.porHandle)) {
      if (contribuicao.mod === id) this.revogar(handle);
    }
  }

  /** Tira tudo. A saída da sessão passa por aqui. */
  limpar() {
    this.porPonto.clear();
    this.porHandle.clear();
    this.porMod.clear();
    this.avisar();
  }

  /**
   * As contribuições de um ponto para um alvo, do mais prioritário ao menos.
   *
   * Inclui as que não declararam alvo: uma contribuição para
   * `servidor.navegacao` vale sempre, e uma para `pessoa.cartao` sem alvo vale
   * para toda pessoa — que é como um MOD apresenta todo mundo sem precisar
   * registrar sessenta e quatro vezes.
   */
  para(ponto, alvo = "") {
    const porAlvo = this.porPonto.get(ponto);
    if (!porAlvo) return [];
    const especificas = alvo ? (porAlvo.get(String(alvo)) ?? []) : [];
    const gerais = porAlvo.get("") ?? [];
    if (!especificas.length) return gerais;
    if (!gerais.length) return especificas;
    return [...especificas, ...gerais]
      .sort((a, b) => b.prioridade - a.prioridade || a.ordem - b.ordem);
  }

  /**
   * Quem substitui este ponto, quando alguém substitui.
   *
   * **Uma decisão, e ela é visível.** §6: «Duas substituições do mesmo ponto
   * precisam de escolha determinística e visível: administrador escolhe o
   * provedor do servidor, com preferência local quando cabível. Nada de
   * "última resposta assíncrona vence".»
   *
   * A escolha aqui é: a de maior prioridade; empatadas, a que foi registrada
   * primeiro. A preferência de quem administra entra por `preferido`, que a
   * gestão de MODs escreve — e ela ganha de qualquer prioridade, porque uma
   * pessoa decidiu e um número não.
   *
   * Devolve também as preteridas, para a gestão poder **dizer** que houve
   * disputa em vez de a segunda sumir sem explicação.
   */
  escolherSubstituicao(ponto, alvo = "", preferido = "") {
    const candidatas = this.para(ponto, alvo).filter((c) => c.modo === "substituir");
    if (!candidatas.length) return { escolhida: null, preteridas: [] };
    const doPreferido = preferido
      ? candidatas.find((c) => c.mod === preferido)
      : null;
    const escolhida = doPreferido ?? candidatas[0];
    return {
      escolhida,
      preteridas: candidatas.filter((c) => c !== escolhida),
    };
  }

  /** Quem quer ser avisado quando o registro muda. */
  aoMudar(fn) {
    this.ouvintes.add(fn);
    return () => this.ouvintes.delete(fn);
  }

  /**
   * Avisa, **uma vez por quadro**.
   *
   * Um MOD que registra dez contribuições num laço dispararia dez redesenhos
   * da lista de pessoas. Coalescer é o que impede que registrar seja mais caro
   * que desenhar.
   */
  avisar() {
    if (this.avisando) return;
    this.avisando = true;
    queueMicrotask(() => {
      this.avisando = false;
      for (const ouvinte of this.ouvintes) {
        try {
          ouvinte();
        } catch (falha) {
          console.warn("contribuições: ouvinte falhou", falha);
        }
      }
    });
  }

  /**
   * O que a gestão de MODs precisa mostrar: quem contribui onde, e as disputas.
   *
   * Sem isto, «Usar apresentação padrão» seria um botão para um problema que a
   * pessoa não tem como ver. O §6 pede os dois juntos.
   */
  resumo(preferidos = new Map()) {
    const linhas = [];
    for (const [ponto, porAlvo] of this.porPonto) {
      const regra = PONTOS_DE_CONTRIBUICAO[ponto];
      if (!regra) continue;
      const todas = [...porAlvo.values()].flat();
      const mods = [...new Set(todas.map((c) => c.mod))];
      const disputa = regra.modos.includes("substituir")
        ? this.escolherSubstituicao(ponto, "", preferidos.get(ponto) ?? "")
        : { escolhida: null, preteridas: [] };
      linhas.push({
        ponto,
        mods,
        quantas: todas.length,
        escolhido: disputa.escolhida?.mod ?? "",
        preteridos: [...new Set(disputa.preteridas.map((c) => c.mod))],
      });
    }
    return linhas.sort((a, b) => a.ponto.localeCompare(b.ponto));
  }
}
