// SEELE — os alertas sonoros (0.19): o toque de quem chama, o aviso de quem
// entra e sai da sala, a queda da conexão, a batida na porta.
//
// **Tudo é sintetizado, nenhum arquivo de áudio.** Um oscilador e um ganho por
// nota, com ataque e soltura suaves (seno e triângulo, nunca quadrada nem
// dente de serra): o som de um aviso tem de ser notado sem assustar quem está
// no meio de uma frase. Sem arquivo, não há o que carregar, cachear, licenciar
// nem o que a CSP (`default-src 'self'`) precise liberar — e a mesma ui/ serve
// o desktop (webview do Tauri) e o navegador (o web app) sem uma linha a mais.
//
// **Um contexto só dele**, criado na primeira vez que alguém precisa de som e
// separado do da voz (`voz-web.js`): a voz abre e fecha o dela com a conversa,
// e um alerta que dependesse dele calaria exatamente quando mais importa (a
// queda, o fim da chamada).
//
// **O produto sabe e não conta é o defeito deste repositório**, então o
// silêncio por falta de permissão não é mudo: o navegador (e o WKWebView) podem
// manter o contexto suspenso até um gesto, e um som pedido nessa hora deixa uma
// linha no console, uma vez por som. O contexto é retomado no primeiro toque
// ou tecla; um toque que repete (a ligação) volta a soar sozinho no ciclo
// seguinte, assim que o contexto roda.
//
// As preferências são desta máquina (`localStorage`, `seele.alertas.v1`) e não
// vão ao servidor: quem decide se o celular apita é quem o carrega.

"use strict";

const alertasSonoros = (() => {
  const CHAVE_DAS_PREFERENCIAS = "seele.alertas.v1";
  const VOLUME_PADRAO = 0.6;
  /** Os grupos que a pessoa liga e desliga, na ordem em que a tela os lista. */
  const GRUPOS = Object.freeze(["ligacoes", "sala", "conexao", "porta", "mensagens"]);
  /** A que grupo cada som pertence. Um som fora desta tabela não toca. */
  const GRUPO_DO_SOM = Object.freeze({
    "ligacao-chegando": "ligacoes",
    chamando: "ligacoes",
    conectou: "ligacoes",
    encerrou: "ligacoes",
    "entrou-na-sala": "sala",
    "saiu-da-sala": "sala",
    "conexao-caiu": "conexao",
    "conexao-voltou": "conexao",
    "bateu-a-porta": "porta",
    mensagem: "mensagens",
  });

  /**
   * O catálogo. Cada nota é `[hz, começo, duração, ganho]` (segundos), com
   * `hzFim` quando a altura desliza. O ganho é o pico relativo, e fica longe de
   * 1: o volume da pessoa multiplica por cima, e a soma de duas notas que se
   * sobrepõem não pode estourar.
   *
   * - `laco`: o som que se repete até alguém pará-lo: `periodo` é o intervalo
   *   entre os começos de duas frases e `teto` o limite duro, em ms. Sem teto
   *   um toque esquecido (a pessoa saiu da mesa) tocaria a noite inteira.
   * - `minimo`: o intervalo mínimo entre dois disparos do mesmo som, em
   *   segundos do relógio do áudio, para uma rajada de mensagens não virar
   *   uma metralhadora.
   */
  const SONS = Object.freeze({
    // Duas notas (sol, mi) duas vezes: 1,2 s de som e 0,8 s de pausa.
    "ligacao-chegando": {
      tipo: "triangle",
      notas: [[783.99, 0, 0.26, 0.3], [659.25, 0.3, 0.26, 0.3], [783.99, 0.6, 0.26, 0.3], [659.25, 0.9, 0.3, 0.3]],
      laco: { periodo: 2000, teto: 45000 },
    },
    // O «chamando» de quem liga: um tom macio de 1 s, a cada 4 s.
    chamando: {
      tipo: "sine",
      notas: [[440, 0, 1, 0.22]],
      laco: { periodo: 4000, teto: 60000 },
    },
    conectou: { tipo: "sine", notas: [[523.25, 0, 0.18, 0.3], [783.99, 0.13, 0.34, 0.3]] },
    encerrou: { tipo: "sine", notas: [[587.33, 0, 0.18, 0.3], [392, 0.13, 0.4, 0.3]] },
    // Um sopro: a altura sobe (entrou) ou desce (saiu) em 0,12 s.
    "entrou-na-sala": { tipo: "sine", notas: [[520, 0, 0.12, 0.22, 700]], minimo: 0.3 },
    "saiu-da-sala": { tipo: "sine", notas: [[700, 0, 0.12, 0.22, 500]], minimo: 0.3 },
    "conexao-caiu": { tipo: "triangle", notas: [[196, 0, 0.22, 0.34], [146.83, 0.26, 0.34, 0.34]] },
    // Da família do «conectou» (dois sinos subindo), uma quinta acima de onde ele começa.
    "conexao-voltou": { tipo: "sine", notas: [[587.33, 0, 0.18, 0.3], [880, 0.13, 0.34, 0.3]] },
    // Duas batidas curtas e graves; a altura cai um pouco, como madeira.
    "bateu-a-porta": { tipo: "sine", notas: [[170, 0, 0.1, 0.55, 110], [170, 0.2, 0.1, 0.55, 110]] },
    mensagem: { tipo: "sine", notas: [[880, 0, 0.12, 0.2]], minimo: 1 },
  });

  const ATAQUE = 0.015;
  const SOLTURA = 0.06;
  /** Quanto o som leva para sumir quando um laço é parado no meio de uma nota. */
  const SUMIR = 0.04;

  // ------------------------------------------------------ preferências

  function padroes() {
    return { ligado: true, volume: VOLUME_PADRAO, grupos: Object.fromEntries(GRUPOS.map((g) => [g, true])) };
  }

  /** O que veio do disco, ou de quem chama, reduzido ao que é válido. */
  function sanear(bruto, base) {
    const saida = { ligado: base.ligado, volume: base.volume, grupos: { ...base.grupos } };
    if (!bruto || typeof bruto !== "object") return saida;
    if (typeof bruto.ligado === "boolean") saida.ligado = bruto.ligado;
    if (typeof bruto.volume === "number" && Number.isFinite(bruto.volume)) {
      saida.volume = Math.min(1, Math.max(0, bruto.volume));
    }
    if (bruto.grupos && typeof bruto.grupos === "object") {
      for (const grupo of GRUPOS) if (typeof bruto.grupos[grupo] === "boolean") saida.grupos[grupo] = bruto.grupos[grupo];
    }
    return saida;
  }

  function lerDoDisco() {
    try {
      const texto = localStorage.getItem(CHAVE_DAS_PREFERENCIAS);
      return texto ? sanear(JSON.parse(texto), padroes()) : padroes();
    } catch {
      // Sem armazenamento (navegação privada) ou JSON quebrado: os padrões.
      return padroes();
    }
  }

  /** Lido uma vez e guardado: uma gravação que falha não desfaz a escolha da sessão. */
  let atuais = null;
  function preferencias() {
    atuais ??= lerDoDisco();
    return { ligado: atuais.ligado, volume: atuais.volume, grupos: { ...atuais.grupos } };
  }

  /** Muda o que foi dito (o resto fica), grava, e devolve as preferências de agora. */
  function definir(mudanca) {
    atuais = sanear(mudanca, atuais ?? lerDoDisco());
    try {
      localStorage.setItem(CHAVE_DAS_PREFERENCIAS, JSON.stringify(atuais));
    } catch {
      /* A escolha vale nesta página; a próxima abertura volta aos padrões. */
    }
    if (mestre) mestre.gain.value = atuais.volume;
    return preferencias();
  }

  function permitido(nome) {
    const grupo = GRUPO_DO_SOM[nome];
    if (!grupo) return false;
    const agora = atuais ?? (atuais = lerDoDisco());
    return agora.ligado && agora.grupos[grupo] === true;
  }

  // ----------------------------------------------------------- o contexto

  let contexto = null;
  let mestre = null;
  /** A saída que o contexto já tem (`""` é a padrão); `null` depois de uma recusa, para tentar de novo. */
  let saidaAplicada = "";
  /** A página está indo embora: nada novo começa. `pageshow` (bfcache) desfaz. */
  let descarregando = false;
  const jaContados = new Set();

  /** Deixa a linha que diz que um som não tocou, uma vez por som. */
  function contarSilencio(nome, motivo) {
    if (jaContados.has(nome)) return;
    jaContados.add(nome);
    console.info(`alertas sonoros: «${nome}» não tocou: ${motivo}`);
  }

  function obterContexto() {
    if (contexto) return contexto;
    const Construtor = globalThis.AudioContext || globalThis.webkitAudioContext;
    if (typeof Construtor !== "function") return null;
    try {
      contexto = new Construtor();
      mestre = contexto.createGain();
      mestre.gain.value = preferencias().volume;
      mestre.connect(contexto.destination);
    } catch (falha) {
      contexto = null;
      mestre = null;
      console.info("alertas sonoros: o navegador não abriu o áudio:", falha);
    }
    return contexto;
  }

  /**
   * A saída que a pessoa escolheu para a voz, quando o navegador deixa o
   * contexto ir para lá. A chave é a da ponte (`seele.saida`, lida por
   * `guardadoNaOrigem`), e não uma cópia dela: duas cópias discordariam. No
   * desktop a escolha mora no Rust com um id nativo que o navegador não
   * entende, então ali a chave está vazia e o alerta sai pela saída padrão.
   */
  function aplicarASaida(ctx) {
    if (typeof ctx.setSinkId !== "function") return;
    let id = "";
    try {
      if (typeof guardadoNaOrigem === "function" && typeof CHAVES_DA_ORIGEM === "object") {
        id = guardadoNaOrigem(CHAVES_DA_ORIGEM.saida) || "";
      }
    } catch {
      id = "";
    }
    if (id === saidaAplicada) return;
    saidaAplicada = id;
    try {
      Promise.resolve(ctx.setSinkId(id)).catch(() => {
        saidaAplicada = null;
      });
    } catch {
      saidaAplicada = null;
    }
  }

  /** O contexto, se ele está rodando agora; senão tenta retomá-lo e diz que não. */
  function contextoRodando(nome) {
    const ctx = obterContexto();
    if (!ctx) {
      contarSilencio(nome, "este navegador não tem Web Audio");
      return null;
    }
    if (ctx.state !== "running") {
      // Sem gesto da pessoa o navegador recusa; com um, isto basta.
      try {
        Promise.resolve(ctx.resume()).catch(() => {});
      } catch {
        /* O `resume` pode não existir num contexto muito antigo. */
      }
      contarSilencio(nome, `o áudio do navegador está ${ctx.state}, à espera de um toque ou tecla`);
      return null;
    }
    aplicarASaida(ctx);
    return ctx;
  }

  // ------------------------------------------------------------- as notas

  /** Todo som em curso (um laço ou um disparo): é o que `pararTudo` silencia. */
  const emCurso = new Set();

  /** Agenda as notas de um som no contexto, ligadas a `barramento`. */
  function agendar(ctx, som, barramento, naFim) {
    const inicio = ctx.currentTime;
    let ultimo = null;
    for (const [hz, comeco, duracao, ganho, hzFim] of som.notas) {
      const osc = ctx.createOscillator();
      const envelope = ctx.createGain();
      osc.type = som.tipo;
      osc.frequency.setValueAtTime(hz, inicio + comeco);
      if (hzFim) osc.frequency.linearRampToValueAtTime(hzFim, inicio + comeco + duracao);
      const ataque = Math.min(ATAQUE, duracao / 3);
      const soltura = Math.min(SOLTURA, duracao / 2);
      envelope.gain.setValueAtTime(0, inicio + comeco);
      envelope.gain.linearRampToValueAtTime(ganho, inicio + comeco + ataque);
      envelope.gain.setValueAtTime(ganho, inicio + comeco + duracao - soltura);
      envelope.gain.linearRampToValueAtTime(0, inicio + comeco + duracao);
      osc.connect(envelope);
      envelope.connect(barramento.no);
      osc.start(inicio + comeco);
      osc.stop(inicio + comeco + duracao + 0.02);
      barramento.osciladores.add(osc);
      osc.onended = () => barramento.osciladores.delete(osc);
      ultimo = osc;
    }
    if (ultimo && naFim) ultimo.addEventListener?.("ended", naFim);
  }

  /** Um barramento com o seu próprio ganho: pará-lo cala só o que passa por ele. */
  function criarBarramento(ctx) {
    const no = ctx.createGain();
    no.gain.value = 1;
    no.connect(mestre);
    return { no, osciladores: new Set() };
  }

  function calar(ctx, barramento) {
    const agora = ctx.currentTime;
    try {
      barramento.no.gain.cancelScheduledValues(agora);
      barramento.no.gain.setValueAtTime(barramento.no.gain.value, agora);
      barramento.no.gain.linearRampToValueAtTime(0, agora + SUMIR);
      for (const osc of barramento.osciladores) osc.stop(agora + SUMIR + 0.01);
    } catch {
      /* Um oscilador que já parou recusa o segundo `stop`: o som já acabou. */
    }
  }

  const ultimoDisparo = new Map();

  /**
   * Toca o som uma vez. Devolve `true` se ele foi agendado. `teste` é o botão
   * TESTAR das configurações: ele existe para a pessoa ouvir o volume, e por
   * isso ignora o interruptor e os grupos (um teste mudo diria que quebrou).
   */
  function tocar(nome, { teste = false } = {}) {
    const som = SONS[nome];
    if (!som || descarregando || !(teste || permitido(nome))) return false;
    const ctx = contextoRodando(nome);
    if (!ctx) return false;
    const agora = ctx.currentTime;
    if (som.minimo && agora - (ultimoDisparo.get(nome) ?? -Infinity) < som.minimo) return false;
    ultimoDisparo.set(nome, agora);
    try {
      const barramento = criarBarramento(ctx);
      const dono = { calar: () => calar(ctx, barramento) };
      emCurso.add(dono);
      agendar(ctx, som, barramento, () => emCurso.delete(dono));
      return true;
    } catch (falha) {
      console.info(`alertas sonoros: «${nome}» falhou ao tocar:`, falha);
      return false;
    }
  }

  // ---------------------------------------------------------------- laços

  /** Os laços vivos, por nome: começar o mesmo duas vezes devolve o que já há. */
  const lacos = new Map();

  function repetir(nome) {
    const som = SONS[nome];
    if (!som?.laco) throw new Error(`alertas sonoros: «${nome}» não é um som que se repete`);
    const vivo = lacos.get(nome);
    if (vivo) return vivo;

    let barramento = null;
    let proximo = null;
    let teto = null;
    let parado = false;
    const dono = { calar: () => parar() };

    function parar() {
      // Idempotente: um laço parado duas vezes (o fim da chamada e o teto)
      // não pode calar o que começou depois com o mesmo nome.
      if (parado) return;
      parado = true;
      clearTimeout(proximo);
      clearTimeout(teto);
      if (lacos.get(nome) === handle) lacos.delete(nome);
      emCurso.delete(dono);
      if (barramento && contexto) calar(contexto, barramento);
    }

    function ciclo() {
      if (parado) return;
      // A cada ciclo, e não só no começo: a pessoa que desliga os alertas com
      // o telefone tocando é atendida no ciclo seguinte, e o contexto que um
      // toque acabou de acordar começa a soar sem ninguém pedir de novo.
      if (!descarregando && permitido(nome)) {
        const ctx = contextoRodando(nome);
        if (ctx) {
          try {
            barramento ??= criarBarramento(ctx);
            agendar(ctx, som, barramento);
          } catch (falha) {
            console.info(`alertas sonoros: «${nome}» falhou ao tocar:`, falha);
          }
        }
      }
      proximo = setTimeout(ciclo, som.laco.periodo);
    }

    const handle = Object.freeze({ parar });
    lacos.set(nome, handle);
    emCurso.add(dono);
    teto = setTimeout(() => {
      console.info(`alertas sonoros: «${nome}» parou no teto de ${som.laco.teto / 1000} s`);
      parar();
    }, som.laco.teto);
    ciclo();
    return handle;
  }

  /** Para o laço com este nome, se houver. */
  function parar(nome) {
    lacos.get(nome)?.parar();
  }

  /** Quem pediu cada laço pelo `manter`: o nome do som → os donos que o querem ligado. */
  const quemMantem = new Map();

  /**
   * Mantém o laço ligado ou desligado conforme `ativo`, idempotente nos dois
   * sentidos. **Por dono** (`quem`): o toque da sala pessoal e o da conversa
   * por link usam o mesmo `ligacao-chegando`, e um que para não cala o outro.
   * O laço só para quando nenhum dono o quer mais.
   */
  function manter(nome, ativo, quem = "") {
    const donos = quemMantem.get(nome) ?? new Set();
    if (ativo) donos.add(quem);
    else donos.delete(quem);
    if (donos.size) {
      quemMantem.set(nome, donos);
      repetir(nome);
    } else {
      quemMantem.delete(nome);
      parar(nome);
    }
  }

  function pararTudo() {
    for (const dono of [...emCurso]) dono.calar();
    emCurso.clear();
    quemMantem.clear();
  }

  // ------------------------------------------------- o que os desenhos veem

  /**
   * A conversa por link: as fases que `pessoal-web-entrada.js` conhece. O som
   * segue a transição, e não a fase: chamar `notarFase` de novo com a mesma
   * fase não toca nada.
   */
  function notarFase(anterior, atual) {
    if (anterior === atual) return;
    manter("ligacao-chegando", atual === "tocando", "link");
    manter("chamando", atual === "chamando", "link");
    if (atual === "dentro") tocar("conectou");
    else if (atual === "encerrada" && (anterior === "dentro" || anterior === "chamando")) tocar("encerrou");
  }

  /** As pessoas de OUTRO que não eu na sala de voz em que estou, por id; `null` sem sala. */
  function visitantes(retrato) {
    const sala = retrato?.voice_rooms?.find((s) => s.occupied_by_us);
    if (!sala) return null;
    return { sala: sala.id, ids: new Set((sala.people ?? []).filter((p) => !p.is_self).map((p) => p.id)) };
  }

  /**
   * Quem entrou e quem saiu da sala de voz em que estou, de um retrato para o
   * seguinte. Só vale **dentro da mesma sala**: o primeiro retrato depois de
   * entrar (a lista inteira «chega» de uma vez) e a troca de sala não são
   * ninguém entrando, e eu mesmo nunca conto.
   */
  function notarSala(anterior, atual) {
    try {
      const antes = visitantes(anterior);
      const agora = visitantes(atual);
      if (!antes || !agora || antes.sala !== agora.sala) return;
      if ([...agora.ids].some((id) => !antes.ids.has(id))) tocar("entrou-na-sala");
      if ([...antes.ids].some((id) => !agora.ids.has(id))) tocar("saiu-da-sala");
    } catch (falha) {
      console.info("alertas sonoros: a sala não pôde ser comparada:", falha);
    }
  }

  const naBateria = (link) => Boolean(link && link !== "Online" && link.InternalBattery);

  /**
   * A conexão, de um retrato para o seguinte: cai uma vez por queda (a faixa
   * da bateria contando não repete) e volta uma vez por recuperação. Sem
   * retrato anterior não há o que comparar: abrir a sessão já caída não
   * «volta» depois como se tivesse caído agora.
   */
  function notarEnlace(anterior, atual) {
    try {
      if (!anterior || !atual) return;
      const antes = naBateria(anterior.link);
      const agora = naBateria(atual.link);
      if (!antes && agora) tocar("conexao-caiu");
      else if (antes && !agora && atual.link === "Online") tocar("conexao-voltou");
    } catch (falha) {
      console.info("alertas sonoros: a conexão não pôde ser comparada:", falha);
    }
  }

  // ----------------------------------------------- gesto e fim da página

  if (typeof document === "object" && document && typeof document.addEventListener === "function") {
    const acordar = () => {
      document.removeEventListener("pointerdown", acordar, true);
      document.removeEventListener("keydown", acordar, true);
      // Dentro do gesto: é aqui que o navegador deixa o contexto nascer rodando.
      const ctx = obterContexto();
      try {
        if (ctx && ctx.state !== "running") Promise.resolve(ctx.resume()).catch(() => {});
      } catch {
        /* Sem `resume`, nada a fazer. */
      }
    };
    document.addEventListener("pointerdown", acordar, true);
    document.addEventListener("keydown", acordar, true);
  }
  if (typeof window === "object" && window && typeof window.addEventListener === "function") {
    // A página indo embora não começa som novo (um toque que nasce no
    // `pagehide` soaria sobre uma tela que já não existe).
    window.addEventListener("pagehide", () => {
      descarregando = true;
      pararTudo();
    });
    window.addEventListener("pageshow", () => {
      descarregando = false;
    });
  }

  return Object.freeze({
    GRUPOS,
    SONS: Object.freeze(Object.keys(SONS)),
    tocar,
    repetir,
    parar,
    manter,
    pararTudo,
    preferencias,
    definir,
    notarFase,
    notarSala,
    notarEnlace,
  });
})();
