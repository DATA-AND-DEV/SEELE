// SEELE — a notificação do sistema: o toque no ombro com a janela fora da
// frente (a do sistema operacional, ou a do navegador na página).
//
// Era «o que sobrou da pendência 23, item 1»: o app só avisava dentro da
// janela, e quem hospedava com a janela atrás de um jogo não ficava sabendo de
// quem batia à porta.
//
// # O que notifica
//
// Três coisas, e a lista é fechada (`MOTIVOS`):
// - **a menção**: o core levantou `Mentioned` numa sessão, a da vista ou
//   qualquer outra;
// - **a batida na porta** de quem hospeda;
// - **a ligação de conversa pessoal chegando**: o toque da sala (`seele://toque`)
//   e o da conversa por link.
//
// Mensagem comum de canal não notifica: seria barulho. Mensagem de conversa
// pessoal também não, por enquanto: a sala não tem um evento que diga «chegou
// uma mensagem de outra pessoa» (o `MessagesChanged` é o mesmo para a própria,
// para a edição e para o histórico).
//
// # Quando
//
// A regra é `decidir`, pura, e é ela que os testes conferem
// (`tests/frontend.rs`). Nesta ordem, uma notificação **não** sai:
// 1. por um motivo fora da lista;
// 2. pelo que a própria pessoa fez (a ligação que ela mesma faz);
// 3. com a preferência desligada — e ela nasce desligada;
// 4. sem a permissão do navegador (negada, ainda por pedir, ou sem a API);
// 5. com a janela na frente: o aviso de dentro basta;
// 6. repetida: a mesma batida, o mesmo toque, ou outra menção no mesmo
//    servidor dentro de meio minuto.
//
// O que a janela já avisou por dentro também conta como dito: quem viu a faixa
// de quem bate e foi para outra janela não é avisado de novo, cinco segundos
// depois, da mesma batida.
//
// «Não me incomode» não tem estado próprio no produto. O ISOLAMENTO TOTAL é
// sobre som (quem o liga não ouve o toque, e é quem mais precisa do aviso que
// se vê), e não cala a notificação; quem cala é esta preferência, o «não
// receber ligações» do perfil (sem sala não há toque) e o Não Perturbe do
// sistema, que as notificações do sistema já respeitam.
//
// # Quem emite
//
// Esta camada só pede, por `invoke`, como todo o resto da casca:
// - no app, `notificar_no_sistema` é um comando do Rust, que confere de novo a
//   preferência no disco e a janela na frente antes de chamar o plugin
//   (`src/notificacao.rs`);
// - na página de navegador, é a ponte (`ponte-web.js`), com a API
//   `Notification`.
//
// **O que este arquivo não sabe, e não afirma**: se a notificação apareceu. O
// plugin do app descarta o resultado da entrega, e nenhuma frase daqui diz
// «notificado».
//
// O clique: no app o sistema só traz a janela à frente. Na página a ponte
// devolve o lugar por `seele://notificacao-clicada`, e esta camada leva à
// sessão da menção.

"use strict";

const notificacaoDoSistema = (() => {
  /** O motivo daqui e o nome dele no Rust (`notificacao::Motivo`). Fora desta tabela, nada sai. */
  const MOTIVOS = Object.freeze({ mencao: "Mencao", porta: "Porta", ligacao: "Ligacao" });

  /**
   * As permissões com que um pedido segue. `NaoSeSabe` é a do app: o plugin
   * não deixa ler o que o sistema decidiu, e quem responde é o sistema.
   */
  const PERMISSOES_QUE_DEIXAM = Object.freeze(["Concedida", "NaoSeSabe"]);

  /** Outra menção no mesmo servidor dentro deste tempo é a mesma notícia. */
  const SILENCIO_DA_MENCAO_MS = 30000;

  /** Quantas chaves a memória guarda antes de esquecer as vencidas e as mais velhas. */
  const TETO_DA_MEMORIA = 400;

  const nao = (porque) => Object.freeze({ notificar: false, porque });

  /**
   * **A regra.** Pura: o que se sabe agora (`estado`) e o que aconteceu
   * (`acontecimento`), e a resposta é se sai uma notificação e, quando não, por
   * quê — pelo nome, que é o que os testes e o console leem.
   *
   * `estado`: `{ ligadas, permissao, naFrente, agora, ditas }`, em que `ditas`
   * é um `Map` da chave ao instante até o qual ela vale como já dita.
   * `acontecimento`: `{ motivo, chaves, propria, validadeMs }`. Com várias
   * chaves (quem bate à porta), é repetição só se **todas** já foram ditas.
   */
  function decidir(estado, acontecimento) {
    const { ligadas, permissao, naFrente, agora, ditas } = estado ?? {};
    const { motivo, chaves, propria, validadeMs } = acontecimento ?? {};
    if (typeof motivo !== "string" || !Object.hasOwn(MOTIVOS, motivo)) return nao("motivo-desconhecido");
    if (!Array.isArray(chaves) || chaves.length === 0) return nao("sem-chave");
    if (propria) return nao("propria");
    if (ligadas !== true) return nao("desligadas");
    if (!PERMISSOES_QUE_DEIXAM.includes(permissao)) return nao("sem-permissao");
    if (naFrente !== false) return nao("janela-na-frente");
    const novas = chaves.filter((chave) => !((ditas?.get(chave) ?? -Infinity) > agora));
    if (novas.length === 0) return nao("repetida");
    return Object.freeze({ notificar: true, novas, lembrarAte: agora + (validadeMs ?? Infinity) });
  }

  // ------------------------------------------------------------ a memória

  /** A chave e até quando ela vale como dita (`Infinity`: até ser esquecida). */
  const ditas = new Map();

  function lembrar(chaves, ate) {
    for (const chave of chaves) {
      ditas.delete(chave);
      ditas.set(chave, ate);
    }
    if (ditas.size <= TETO_DA_MEMORIA) return;
    const agora = Date.now();
    for (const [chave, vale] of ditas) if (vale <= agora) ditas.delete(chave);
    // Um `Map` guarda a ordem de entrada: as primeiras são as mais velhas.
    for (const chave of ditas.keys()) {
      if (ditas.size <= TETO_DA_MEMORIA) break;
      ditas.delete(chave);
    }
  }

  function esquecer(chave) {
    ditas.delete(chave);
  }

  // ------------------------------------------------------- o que se conta

  /** Quantas saíram, e quantas não saíram por cada motivo: para a bancada e para quem investiga. */
  const contas = { pedidas: 0, porque: {} };
  const jaDitoNoConsole = new Set();

  /** Os motivos de todo dia: contados, e não escritos no console. */
  const SILENCIOS_COMUNS = new Set(["desligadas", "janela-na-frente", "repetida", "propria"]);

  function naoSaiu(motivo, porque, falha) {
    contas.porque[porque] = (contas.porque[porque] ?? 0) + 1;
    if (!SILENCIOS_COMUNS.has(porque) && !jaDitoNoConsole.has(`${motivo}/${porque}`)) {
      jaDitoNoConsole.add(`${motivo}/${porque}`);
      console.info(`notificação do sistema: «${motivo}» não saiu: ${porque}`, falha ?? "");
    }
    return Object.freeze({ notificou: false, porque });
  }

  /** O nome de uma recusa do comando: a do Rust é uma palavra, a da ponte um objeto. */
  function nomeDaRecusa(falha) {
    if (typeof falha === "string") return falha;
    if (falha && typeof falha === "object") return Object.keys(falha)[0] ?? "falhou";
    return "falhou";
  }

  // --------------------------------------------------------------- a janela

  /**
   * A janela está na frente: à vista **e** com o foco. Escondida (minimizada,
   * outra aba) ou sem foco (outra janela por cima), não está.
   *
   * Sem conseguir ler, «está»: é a resposta que não notifica. No app o Rust
   * confere o foco de novo pelo sistema antes de emitir.
   */
  function janelaNaFrente() {
    try {
      if (document.visibilityState === "hidden") return false;
      return typeof document.hasFocus === "function" ? document.hasFocus() : true;
    } catch {
      return true;
    }
  }

  // ----------------------------------------------------------------- avisar

  /**
   * Um acontecimento, do começo ao fim: decide, monta o texto e pede.
   *
   * `dados` é o que a frase precisa (`FRASES_DA_NOTIFICACAO.texto[motivo]`), ou
   * uma função que o busca — chamada só quando a notificação vai sair. `lugar`
   * é para onde o clique leva, na página de navegador.
   *
   * Devolve `{ notificou, porque? }`. Nunca lança: um aviso que falha não
   * derruba o desenho da tela que o pediu.
   */
  async function avisar({ motivo, chaves, propria = false, validadeMs, dados = {}, lugar = null } = {}) {
    const acontecimento = { motivo, chaves, propria, validadeMs };
    try {
      // Primeiro o que se sabe sem perguntar a ninguém: com a janela na frente
      // — o caso de quase sempre — não há volta de IPC.
      const naFrente = janelaNaFrente();
      const previa = decidir(
        { ligadas: true, permissao: "NaoSeSabe", naFrente, agora: Date.now(), ditas },
        acontecimento,
      );
      if (!previa.notificar) {
        // O que a janela avisou por dentro está dito: ver o topo deste arquivo.
        if (previa.porque === "janela-na-frente") lembrar(chaves, Date.now() + (validadeMs ?? Infinity));
        return naoSaiu(motivo, previa.porque);
      }
      const retrato = await invoke("estado_das_notificacoes");
      const agora = Date.now();
      const decisao = decidir(
        { ligadas: retrato?.ligadas, permissao: retrato?.permissao, naFrente: janelaNaFrente(), agora, ditas },
        acontecimento,
      );
      if (!decisao.notificar) {
        if (decisao.porque === "janela-na-frente") lembrar(chaves, agora + (validadeMs ?? Infinity));
        return naoSaiu(motivo, decisao.porque);
      }
      // Lembrado antes do pedido: dois avisos do mesmo acontecimento que se
      // cruzam na espera não viram duas notificações.
      lembrar(chaves, decisao.lembrarAte);
      const texto = FRASES_DA_NOTIFICACAO.texto[motivo](typeof dados === "function" ? await dados() : dados);
      try {
        await invoke("notificar_no_sistema", {
          motivo: MOTIVOS[motivo],
          titulo: texto.titulo,
          corpo: texto.corpo,
          // Só a página de navegador usa: o comando do app não o lê.
          lugar,
        });
      } catch (falha) {
        // Não saiu: a mesma notícia pode sair numa próxima vez.
        for (const chave of decisao.novas) esquecer(chave);
        return naoSaiu(motivo, nomeDaRecusa(falha), falha);
      }
      contas.pedidas += 1;
      return Object.freeze({ notificou: true, titulo: texto.titulo, corpo: texto.corpo });
    } catch (falha) {
      return naoSaiu(String(motivo), "falhou", falha);
    }
  }

  // ------------------------------------------------------------- os ganchos

  /** O nome do servidor de uma sessão, pela trilha; `null` sem saber. */
  async function nomeDaSessao(sessao) {
    if (typeof sessao !== "number") return null;
    try {
      const lista = await invoke("sessoes");
      const desta = (Array.isArray(lista) ? lista : []).find((s) => s?.carimbo?.sessao === sessao);
      return desta?.nome ?? null;
    } catch {
      return null;
    }
  }

  /**
   * Um `seele://event` cru, de qualquer sessão. A menção de um servidor que não
   * está na vista é a que mais precisa do aviso, e o `listen` da casca não a
   * entrega às telas (`base.js`): por isso este ouvinte é próprio.
   *
   * O core levanta `Mentioned` uma vez por mensagem, só para o que é dito ao
   * vivo, e nunca para a mensagem da própria pessoa (`seele-sessao`,
   * `state.rs`): o «nunca para o que a própria pessoa fez» da menção mora lá.
   */
  function aoEventoDeSessao(carga) {
    const carimbado = carga && typeof carga === "object" && typeof carga.sessao === "number" && "evento" in carga;
    const evento = carimbado ? carga.evento : carga;
    if (evento?.NoticeRaised?.notice?.reason !== "Mentioned") return null;
    const sessao = carimbado ? carga.sessao : null;
    return avisar({
      motivo: "mencao",
      chaves: [`mencao#${sessao ?? "?"}`],
      validadeMs: SILENCIO_DA_MENCAO_MS,
      dados: async () => ({ servidor: await nomeDaSessao(sessao) }),
      lugar: sessao === null ? null : { sessao },
    });
  }

  /**
   * A porta de quem hospeda, a cada leitura (`camada-portaria.js`,
   * `atualizarPorta`): as chaves de quem espera, cada uma com o servidor, a
   * geração da hospedagem, a impressão e o instante da batida. Uma batida é
   * notificada uma vez; a contagem do texto é de quem espera agora.
   */
  function notarPorta(chaves, servidor) {
    const esperando = Array.isArray(chaves) ? chaves : [];
    if (esperando.length === 0) return null;
    return avisar({
      motivo: "porta",
      chaves: esperando.map((chave) => `porta#${chave}`),
      dados: { quantos: esperando.length, servidor: servidor ?? null },
      lugar: { portaria: true },
    });
  }

  /**
   * O toque da sala pessoal, como chega por `seele://toque` (`camada-toque.js`).
   * A mesma pessoa tocando de novo (outro candidato da corrida) é o mesmo
   * toque; quando ele para, a próxima ligação dela é outra.
   */
  function notarToque(payload) {
    const quem = payload?.quem;
    if (typeof quem !== "string") return null;
    const chave = `ligacao#sala#${quem}`;
    if (payload.evento === "Parou") {
      esquecer(chave);
      return null;
    }
    if (payload.evento !== "Tocou") return null;
    return avisar({
      motivo: "ligacao",
      chaves: [chave],
      dados: { nome: payload.nome_na_agenda ?? null, apelido: payload.apelido ?? null },
      lugar: { toque: true },
    });
  }

  /**
   * A conversa por link, pela fase (`pessoal-web-entrada.js`, `aoEstado`), como
   * o `notarFase` dos alertas sonoros: é a transição que avisa. `tocando` é
   * alguém ligando; `chamando` é a própria pessoa ligando, e passa pela regra
   * como `propria` para ficar contado que não saiu por isso.
   */
  function notarFase(anterior, estado) {
    const atual = estado?.estado;
    if (anterior === atual || (atual !== "tocando" && atual !== "chamando")) return null;
    return avisar({
      motivo: "ligacao",
      chaves: [`ligacao#link#${estado.id ?? "?"}`],
      propria: atual === "chamando",
      dados: { apelido: estado.apelido ?? null },
      lugar: { conversa: true },
    });
  }

  // ------------------------------------------------- a preferência e o clique

  /** A frase do estado, para CONFIGURAÇÕES: uma por permissão, e a de desligadas. */
  function fraseDoEstado(retrato, plataforma) {
    // Onde não há, é isso que a tela diz, com a opção desligada e parada.
    if (retrato?.permissao === "NaoHa") return FRASES_DA_NOTIFICACAO.estado.NaoHa();
    if (!retrato?.ligadas) return FRASES_DA_NOTIFICACAO.estado.desligadas();
    const frase = FRASES_DA_NOTIFICACAO.estado[retrato.permissao];
    if (typeof frase !== "function") return FRASES_DA_NOTIFICACAO.desconhecido;
    return frase(FRASES_DA_NOTIFICACAO.caminho[plataforma] ?? FRASES_DA_NOTIFICACAO.caminho.outro);
  }

  /** O que vale agora: `{ ligadas, permissao }`, ou `null` se ninguém respondeu. */
  async function estado() {
    try {
      return await invoke("estado_das_notificacoes");
    } catch (falha) {
      console.warn("estado_das_notificacoes:", falha);
      return null;
    }
  }

  /**
   * Liga ou desliga, **dentro do gesto**: na página de navegador é aqui que a
   * permissão é pedida, e um pedido fora de um gesto o navegador não mostra (e
   * ensina a recusar). Por isso nada é esperado antes do `invoke`.
   *
   * A confirmação vai junto para a ponte mostrar, e só ela a usa: é a
   * notificação que prova, no navegador, que a página notifica. O app não
   * mostra nenhuma ao ligar (`ligar_notificacoes`, no `main.rs`, diz por quê).
   */
  function definir(ligadas) {
    return invoke("ligar_notificacoes", {
      ligadas: Boolean(ligadas),
      confirmacao: FRASES_DA_NOTIFICACAO.ligadasNaPagina,
    });
  }

  /**
   * O clique numa notificação da página de navegador: a ponte já trouxe a
   * janela à frente, e aqui se vai à sessão da menção. A porta e o toque não
   * pedem viagem: a faixa de cada um fica por cima de qualquer tela.
   */
  function aoClicar(lugar) {
    try {
      if (typeof lugar?.sessao !== "number" || typeof verSessao !== "function") return;
      if (typeof vistaDaJanela === "object" && vistaDaJanela.sessao === lugar.sessao) return;
      Promise.resolve(verSessao(lugar.sessao)).catch((falha) => console.warn("notificação clicada:", falha));
    } catch (falha) {
      console.warn("notificação clicada:", falha);
    }
  }

  if (typeof ouvirNaJanela === "function") {
    Promise.resolve(ouvirNaJanela("seele://event", (mensagem) => aoEventoDeSessao(mensagem?.payload))).catch(() => {});
    // Só a ponte do navegador o emite; no app ninguém, e o ouvinte fica mudo.
    Promise.resolve(ouvirNaJanela("seele://notificacao-clicada", (mensagem) => aoClicar(mensagem?.payload))).catch(() => {});
  }

  return Object.freeze({
    MOTIVOS,
    SILENCIO_DA_MENCAO_MS,
    decidir,
    avisar,
    janelaNaFrente,
    aoEventoDeSessao,
    notarPorta,
    notarToque,
    notarFase,
    fraseDoEstado,
    estado,
    definir,
    aoClicar,
    contas,
  });
})();

// Propriedade do objeto global, como `alertasSonoros`: um `const` do topo de um
// script clássico não vira uma, e os ganchos chamam `globalThis.…?.`.
globalThis.notificacaoDoSistema = notificacaoDoSistema;
