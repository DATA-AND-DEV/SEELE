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
 *   pedindo o mesmo ponto precisam de uma decisão, e ela é tomada aqui.
 *
 * # `decorar` não está nesta versão, e por quê
 *
 * O plano da API previu um terceiro modo: alterar a apresentação de um nó **do
 * produto** sem trocar o conteúdo dele — pôr uma cor num canal sem assumir o
 * desenho do item. A tabela o listou em dois pontos, e `registrar` o aceitou.
 * Nada o aplicava: uma contribuição `decorar` entrava no registro, devolvia
 * handle e não mudava pixel nenhum.
 *
 * O que falta é o **contrato**, e não o esforço. As classes validadas de
 * `mods-estilos.js` são um conjunto só, pensado para o que está **dentro** de
 * uma raiz do MOD: lá, `opacidade: 0` é uma escolha estética sobre o desenho
 * dele. Esse mesmo conjunto, aplicado a um nó do produto, inclui propriedades
 * que somem com o nome que abre a moderação — e o cabeçalho de `mods-estilos`
 * diz que personalização estética «não pode encobrir uma confirmação de
 * confiança».
 *
 * Isso argumenta por um **segundo conjunto**, e não contra a capacidade: cor,
 * tipografia, fundo e borda em pontos determinados não exigem oferecer
 * deslocamento nem opacidade sobre um controle nativo. Escrever quais
 * propriedades, em quais pontos e sobre quais nós é a continuação do
 * requisito, e **continua pendente**.
 *
 * Até esse contrato existir, o modo é **recusado pelo nome**, com a razão
 * junto. A alternativa — mantê-lo na tabela — é a falha silenciosa que a
 * revisão de 20/09/2026 encontrou, e que este repositório paga mais caro que
 * qualquer recusa.
 *
 * `perfil` diz com que orçamento a declaração é montada. Um cartão custa por
 * pessoa; uma página custa uma vez.
 */
const PONTOS_DE_CONTRIBUICAO = Object.freeze({
  "pessoa.identidade": { modos: ["adicionar"], perfil: "cartao", porAlvo: true },
  "pessoa.cartao": { modos: ["substituir", "adicionar"], perfil: "cartao", porAlvo: true },
  "pessoa.detalhes": { modos: ["adicionar"], perfil: "superficie", porAlvo: true },
  "pessoa.acoes": { modos: ["adicionar"], perfil: "cartao", porAlvo: true },
  "canal.item": { modos: ["adicionar"], perfil: "cartao", porAlvo: true },
  "canal.cabecalho": { modos: ["adicionar"], perfil: "cartao", porAlvo: true },
  "compositor.ferramentas": { modos: ["adicionar"], perfil: "cartao", porAlvo: false },
  "sala.acoes": { modos: ["adicionar"], perfil: "cartao", porAlvo: true },
  "servidor.navegacao": { modos: ["adicionar"], perfil: null, porAlvo: false },
  "servidor.aparencia": { modos: ["substituir"], perfil: null, porAlvo: false },
});

/** Quantas contribuições um MOD registra ao mesmo tempo. */
const TETO_DE_CONTRIBUICOES = 128;

/**
 * A escolha que quer dizer «o SEELE desenha, e nenhum MOD».
 *
 * Um valor reservado e não a ausência de valor, e essa é a correção de R5: a
 * ausência já queria dizer outra coisa — «decida por prioridade». Duas
 * intenções diferentes precisam de dois valores diferentes, e uma delas não
 * pode ser «vazio».
 *
 * Um `id` de MOD é `autor/nome` e nunca começa por `:`, então este valor não
 * colide com nenhum.
 */
const NATIVO = ":nativo";

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
      // **A suspensão é dita pelo nome dela.** Um MOD que peça `decorar` num
      // ponto que hoje só aceita `adicionar` leria «aceita adicionar, e veio
      // decorar» e concluiria que errou o ponto. Ele não errou: o modo existe
      // no plano, e não existe nesta versão.
      if (modo === "decorar") {
        throw new Error(
          "«decorar» não está na API 4: ele mudaria a apresentação de um nó do "
          + "SEELE, e o subconjunto de estilo que impede um MOD de encobrir o "
          + `nome que abre a moderação ainda não existe. Em «${ponto}», use `
          + "«adicionar» — o conteúdo entra ao lado do nativo, sem cobri-lo",
        );
      }
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
      listarNaBarra: pedido?.listarNaBarra !== false,
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
    //
    // O descartador devolvido é guardado na própria contribuição: sem ele, a
    // revogação tirava a contribuição do registro e **deixava o descartador**
    // na instância, segurando o conteúdo declarado até a sessão acabar. A
    // revisão de 20/09/2026 mediu mil ciclos de registrar/revogar terminando
    // com zero contribuições vivas e mil descartadores retidos.
    contribuicao.esquecer = instancia?.registrar(
      `${mod.id}: contribuição ${ponto}`,
      () => this.tirar(contribuicao),
    );
    this.avisar();
    return { handle: contribuicao.handle };
  }

  /**
   * Tira uma contribuição **de quem a registrou**.
   *
   * # Por que o dono é conferido aqui
   *
   * A revisão de 20/09/2026 passou ao roteador uma contribuição de A como
   * pedido de B, e ela foi removida com sucesso. Os handles são sequenciais:
   * `c1`, `c2`, `c3`. Adivinhar o de outro MOD não exige nada.
   *
   * Não é fuga do QuickJS nem acesso ao sistema — é interferência entre MODs
   * pela API do produto, que é precisamente o que o isolamento por servidor
   * existe para não ter.
   *
   * `dono` é o par `(id, instancia)` de quem pediu, vindo do roteador e não do
   * corpo da mensagem. Um MOD não nomeia a si mesmo num campo.
   *
   * @param {string} handle O que `registrar` devolveu.
   * @param {object|null} dono `{ id, instancia }`, ou `null` para a limpeza
   *   interna do produto — ver `revogarDoMod` e `limpar`.
   * @returns {boolean} Se havia o que tirar.
   */
  revogar(handle, dono = null) {
    const contribuicao = this.porHandle.get(handle);
    if (!contribuicao) return false;
    if (dono) {
      // **Três perguntas, e as três precisam ser feitas.** O identificador
      // sozinho não basta: um MOD recarregado dentro da mesma sessão é outra
      // instância, e a contribuição da anterior não é dele.
      const meu = contribuicao.mod === dono.id
        && contribuicao.instancia === dono.instancia
        && contribuicao.geracao === (dono.instancia?.geracao ?? -1);
      if (!meu) {
        throw new Error(
          `a contribuição «${handle}» não é de ${dono.id}: um MOD revoga o que `
          + "ele registrou, e a saída da sessão limpa o resto",
        );
      }
    }
    // Pelo descartador que a instância devolveu: é ele que tira a entrada da
    // lista de recursos **e** chama `tirar`. Chamar `tirar` direto deixaria o
    // descartador retido, que é o defeito que este par existe para fechar.
    if (contribuicao.esquecer) contribuicao.esquecer();
    else this.tirar(contribuicao);
    return true;
  }

  /**
   * Tira a contribuição das tabelas. **Não** mexe na lista de recursos.
   *
   * Separada de `revogar` porque ela é o que o descartador faz, e o descartador
   * é chamado de dois lugares: da revogação pedida pelo MOD, e do encerramento
   * da instância. Uma função que fizesse as duas coisas se chamaria de si
   * mesma por um dos dois caminhos.
   */
  tirar(contribuicao) {
    if (!this.porHandle.delete(contribuicao.handle)) return;
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
    // **E o que foi montado a partir dela sai junto.**
    //
    // Quem monta é a janela — `montarContribuicao` em `base.js` —, e ela deixa
    // aqui uma função para desligar cada destino. Sem esta chamada, revogar
    // tirava o registro lógico e deixava o renderer, o nó e o descartador de
    // cada montagem retidos na instância até a sessão acabar.
    //
    // A revisão de 26ad0c2 encontrou exatamente isso, e explica por que a
    // medida anterior não pegou: mil ciclos de uma contribuição **sem
    // conteúdo montado** não passam por este caminho.
    //
    // Idempotente pelos dois lados: cada descartador de montagem tem a própria
    // guarda, e o campo é zerado aqui.
    const soltarMontagem = contribuicao.soltarMontagem;
    contribuicao.soltarMontagem = null;
    try {
      soltarMontagem?.();
    } catch (falha) {
      console.warn(`MOD ${contribuicao.mod}: o conteúdo de ${contribuicao.ponto} não saiu`, falha);
    }
    // **O conteúdo sai junto.** Ele é o que a closure do descartador segurava,
    // e zerá-lo aqui é o que faz uma referência esquecida no meio do caminho
    // não segurar uma árvore inteira.
    contribuicao.conteudo = null;
    contribuicao.classes = null;
    contribuicao.esquecer = null;
    contribuicao.montadas = null;
    this.avisar();
  }

  /**
   * Tira tudo o que um MOD tem de pé.
   *
   * Limpeza **interna** do produto: ela roda ao descarregar um MOD, e por isso
   * não confere dono — o dono é o produto. É a operação privilegiada que a
   * revisão pediu para separar da pública.
   */
  revogarDoMod(id) {
    for (const [handle, contribuicao] of Array.from(this.porHandle)) {
      if (contribuicao.mod === id) this.revogar(handle);
    }
  }

  /**
   * Tira tudo. A saída da sessão passa por aqui.
   *
   * Pelos descartadores, e não limpando as tabelas: limpar as tabelas deixava
   * os descartadores na instância, retendo o conteúdo de cada contribuição até
   * ela ser encerrada.
   */
  limpar() {
    for (const contribuicao of Array.from(this.porHandle.values())) {
      if (contribuicao.esquecer) contribuicao.esquecer();
      else this.tirar(contribuicao);
    }
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
    if (!candidatas.length) return { escolhida: null, preteridas: [], nativa: false };

    // **Três estados, e eles não se confundem** — R5 da revisão de 20/09/2026.
    //
    // «"Usar apresentação padrão" volta à seleção automática de MODs, não ao
    // cartão nativo.» Havia dois estados onde precisava haver três: com
    // preferência, escolhe aquele MOD; sem preferência, escolhe o primeiro
    // candidato. «Nenhum» não existia — apagar a preferência voltava para o
    // automático, que é justamente o que a pessoa acabou de recusar.
    //
    // Agora:
    //
    // - `""` — **automático**: a prioridade decide, e é o padrão de quem nunca
    //   escolheu nada;
    // - `NATIVO` — **nativo explícito**: nenhuma substituição se aplica, e o
    //   produto desenha o que ele desenharia sem MOD nenhum;
    // - um `id` — **aquele provedor**, se ele estiver de pé.
    if (preferido === NATIVO) {
      return { escolhida: null, preteridas: candidatas, nativa: true };
    }

    // Um provedor escolhido que não está mais de pé — o MOD foi desligado, ou
    // esta sessão é de outro servidor — **não** cai no automático em silêncio:
    // a escolha continua registrada, e quem a fez decide de novo se quiser. O
    // que o produto faz enquanto isso é o nativo, que é a escolha conservadora.
    const doPreferido = preferido
      ? candidatas.find((c) => c.mod === preferido)
      : null;
    if (preferido && !doPreferido) {
      return { escolhida: null, preteridas: candidatas, nativa: true, ausente: preferido };
    }
    const escolhida = doPreferido ?? candidatas[0];
    return {
      nativa: false,
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
      const escolha = preferidos.get(ponto) ?? "";
      const disputa = regra.modos.includes("substituir")
        ? this.escolherSubstituicao(ponto, "", escolha)
        : { escolhida: null, preteridas: [], nativa: false };
      linhas.push({
        ponto,
        mods,
        quantas: todas.length,
        escolhido: disputa.escolhida?.mod ?? "",
        preteridos: [...new Set(disputa.preteridas.map((c) => c.mod))],
        // **Os três estados, para a gestão poder desenhá-los** — R5.
        //
        // `escolhido` vazio significava duas coisas: «ninguém substitui» e
        // «a substituição está desligada». A tela dizia a mesma frase para as
        // duas, e o botão de voltar ao padrão não tinha como dizer se já
        // estava no padrão.
        nativa: disputa.nativa === true,
        automatica: escolha === "",
        // O provedor escolhido que não está mais de pé. A escolha continua
        // registrada — desligar um MOD não é desescolhê-lo —, e a tela precisa
        // poder dizer isso em vez de mostrar «automático».
        ausente: disputa.ausente ?? "",
      });
    }
    return linhas.sort((a, b) => a.ponto.localeCompare(b.ponto));
  }
}
