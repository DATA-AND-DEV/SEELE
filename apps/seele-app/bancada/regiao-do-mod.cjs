// A região de um MOD contra o código de verdade: foco, limites e descarte.
//
// Irmão de `ciclo-do-executor.cjs`, e pelas mesmas razões: `testes/` guarda
// vetores e este é um instrumento; a bateria do Rust lê arquivos e um guarda
// de texto prova que a linha existe, não que ela funciona.
//
// # O que só se prova aqui
//
// **O foco.** «Não reescrever o valor de quem tem foco» é uma linha que um
// guarda de texto confere; que a reconciliação inteira não tire o nó do
// documento no caminho, não. A diferença entre as duas é a diferença entre um
// campo em que se digita e um em que a segunda letra apaga a primeira.
//
// **O descarte.** Que cada `montar` chame `guardar` é forma. Que sair de uma
// sessão **pare o som** e **solte os bytes** é comportamento, e é o pior caso
// que a sonda E1 descreveu.
//
// # O DOM de mentira
//
// Pequeno de propósito: só o que `mods-regiao.js` toca. Um DOM completo
// esconderia o que este arquivo existe para medir — se a região depender de
// algo que não está aqui, ela lança, e lançar é a resposta certa.

const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const raiz = path.resolve(__dirname, "..");
// **Os dois arquivos, na ordem da página.** A região chama `classesDeMod` e
// `folhaDeClassesDeMod`, que moram em `mods-estilos.js` desde a API 4 — e a
// janela os carrega antes por isso mesmo (ver a ordem dos `<script>` no
// `index.html`). Carregar só a região aqui fazia esta bancada reprovar com
// «classesDeMod is not defined», que fala da bancada e não do produto.
const fonte = `${fs.readFileSync(path.join(raiz, "ui/mods-estilos.js"), "utf8")}\n`
  + fs.readFileSync(path.join(raiz, "ui/mods-regiao.js"), "utf8");

const falhas = [];
function confere(caso, condicao, detalhe) {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
}
const volta = () => new Promise((r) => setImmediate(r));

/** Deixa assentar as voltas encadeadas de uma mídia: bytes, decodificação, montagem. */
async function assentar() {
  for (let i = 0; i < 12; i += 1) await volta();
}

// **O fim é dito, e não suposto.** Uma promessa que nunca se resolve esvazia o
// laço de eventos sem erro nenhum: o node sai com 0, sem uma linha, e o passo
// do CI fica verde sem ter provado nada — medido com `await new Promise(() => {})`
// no começo de uma prova. Esta bancada sai por `process.exit` quando chega ao
// fim, e o `beforeExit` só acontece quando o laço esvazia sozinho: é exatamente
// o caso em que ela não chegou lá. O `terminou` vale mesmo assim: se o
// `process.exit(0)` der lugar a um fim natural, é ele que separa o fim de
// verdade da promessa pendurada.
let terminou = false;
process.on("beforeExit", () => {
  if (!terminou) {
    console.error("regiao-do-mod: não chegou ao fim — uma promessa ficou pendurada");
    process.exitCode = 1;
  }
});

// O resumo limpa suas propriedades ao sair; valores inválidos são recusados.
{
  const ctx = vm.createContext({});
  vm.runInContext(fs.readFileSync(path.join(raiz, "ui/mods-estilos.js"), "utf8"), ctx);
  const propriedades = new Map();
  const no = { dataset: {}, style: {
    setProperty: (chave, valor) => propriedades.set(chave, valor),
    removeProperty: chave => propriedades.delete(chave),
  } };
  ctx.aplicarEstiloDeMod(no, { linhasMaximas: 2, direcao: "linha" });
  confere("resumo", propriedades.get("display") === "-webkit-box"
    && propriedades.get("-webkit-line-clamp") === "2", "o layout anulou o resumo");
  ctx.aplicarEstiloDeMod(no, { cor: "#ffffff" });
  confere("resumo removido", !propriedades.has("-webkit-line-clamp") && !propriedades.has("overflow"), "o recorte continuou após remover o estilo");
  for (const linhas of [0, -1, 21, 1.5, "2", Infinity]) {
    const conta = { recusados: 0 };
    const pares = ctx.estiloDeMod({ linhasMaximas: linhas }, conta);
    confere("resumo inválido", conta.recusados === 1 && pares.length === 0, "valor inválido não foi recusado");
  }
}

// --------------------------------------------------------------- o DOM falso

const TEXTO = 3;
const ELEMENTO = 1;

class NoDeTexto {
  constructor(data) {
    this.nodeType = TEXTO;
    this.data = data;
    this.pai = null;
  }
  get nextSibling() {
    if (!this.pai) return null;
    const i = this.pai.filhos.indexOf(this);
    return this.pai.filhos[i + 1] ?? null;
  }
  remove() {
    this.pai?.tirar(this);
  }
}

class Elemento {
  constructor(tag, doc) {
    this.nodeType = ELEMENTO;
    this.tagName = String(tag).toUpperCase();
    this.doc = doc;
    this.filhos = [];
    this.pai = null;
    this.dataset = {};
    this.atributos = new Map();
    this.className = "";
    this.style = {};
    this.ouvintes = new Map();
    this.value = "";
    this.width = 0;
    this.height = 0;
    /** Foi tirado do documento alguma vez? É o que o foco mede. */
    this.saiuDoDocumento = 0;
  }
  get children() {
    return this.filhos.filter((f) => f.nodeType === ELEMENTO);
  }
  /**
   * Está no documento? Sobe pelos pais até o `<body>` desta bancada.
   *
   * É a pergunta que separa um som na tela de um que saiu dela sem ser
   * descartado — a página fechada, o cartão que o SEELE deixou de desenhar.
   */
  get isConnected() {
    let no = this;
    while (no.pai) no = no.pai;
    return no === this.doc?.body;
  }
  get firstChild() {
    return this.filhos[0] ?? null;
  }
  get nextSibling() {
    if (!this.pai) return null;
    const i = this.pai.filhos.indexOf(this);
    return this.pai.filhos[i + 1] ?? null;
  }
  append(...nos) {
    for (const no of nos) this.inserir(no, null);
  }
  insertBefore(no, antes) {
    this.inserir(no, antes);
    return no;
  }
  inserir(no, antes) {
    if (no.pai) no.pai.tirar(no);
    const i = antes ? this.filhos.indexOf(antes) : -1;
    if (i >= 0) this.filhos.splice(i, 0, no);
    else this.filhos.push(no);
    no.pai = this;
  }
  tirar(no) {
    const i = this.filhos.indexOf(no);
    if (i < 0) return;
    this.filhos.splice(i, 1);
    no.pai = null;
    if (no.nodeType !== ELEMENTO) return;
    // **É aqui que o foco morre num navegador de verdade**, e ele morre para a
    // subárvore inteira: tirar um `<label>` do documento tira a `<input>` que
    // está dentro dele, mesmo o pai dela não tendo mudado.
    //
    // A primeira versão só olhava o nó removido, e por isso passava com a
    // reconciliação revertida — o `<label>` saía e voltava, a `<input>` ficava
    // «no lugar», e o guarda não via nada. Um guarda que não falha com o
    // defeito presente não é um guarda.
    const percorrer = (raiz) => {
      raiz.saiuDoDocumento += 1;
      if (this.doc?.activeElement === raiz) this.doc.activeElement = null;
      for (const filho of raiz.children) percorrer(filho);
    };
    percorrer(no);
  }
  remove() {
    this.pai?.tirar(this);
  }
  querySelector(seletor) {
    const classe = seletor.replace(".", "");
    for (const filho of this.children) {
      if (filho.className.split(/\s+/).includes(classe)) return filho;
      const fundo = filho.querySelector(seletor);
      if (fundo) return fundo;
    }
    return null;
  }
  addEventListener(nome, fn) {
    if (!this.ouvintes.has(nome)) this.ouvintes.set(nome, new Set());
    this.ouvintes.get(nome).add(fn);
  }
  removeEventListener(nome, fn) {
    this.ouvintes.get(nome)?.delete(fn);
  }
  disparar(nome, evento = {}) {
    for (const fn of this.ouvintes.get(nome) ?? []) fn(evento);
  }
  /** Quantos ouvintes ainda estão presos neste nó, somando os nomes. */
  ouvintesDePe() {
    let total = 0;
    for (const conjunto of this.ouvintes.values()) total += conjunto.size;
    for (const filho of this.children) total += filho.ouvintesDePe();
    return total;
  }
  // O `<audio>` de mentira saiu com o `<audio>` da região, na fase M1 da
  // casca: o som toca por WebAudio, e quem o imita é `audioDeMentira`. Um
  // `play()` que voltasse a ser chamado aqui lança, que é a resposta certa.
  setAttribute(nome, valor) { this.atributos.set(nome, String(valor)); }
  getAttribute(nome) { return this.atributos.get(nome) ?? null; }
  removeAttribute(nome) {
    this.atributos.delete(nome);
    if (nome === "src") this.src = undefined;
  }
  getContext() {
    return this.pincel;
  }
  /** Um pincel de mentira que anota o que foi pintado, em ordem. */
  darPincel() {
    const pintado = [];
    this.pincel = {
      pintado,
      save() {}, restore() {}, clearRect() {}, beginPath() {},
      moveTo() {}, lineTo() {}, stroke() { pintado.push("traco"); },
      arc() {}, fill() { pintado.push("circulo"); },
      fillRect(...a) { pintado.push(["retangulo", ...a]); },
      strokeRect(...a) { pintado.push(["retangulo-vazado", ...a]); },
      fillText(texto) { pintado.push(["texto", texto]); },
      measureText: (texto) => ({ width: String(texto).length * 7 }),
      font: "", textBaseline: "", fillStyle: "", strokeStyle: "",
      lineWidth: 0, lineCap: "", lineJoin: "",
    };
    return this.pincel;
  }
  getBoundingClientRect() {
    return { left: 0, top: 0, width: this.width || 1, height: this.height || 1 };
  }
  setPointerCapture() {}
}

// ---------------------------------------------------- o WebAudio de mentira

/**
 * Um `AudioContext` de mentira, com as peças que `TocadorDeSomDeMod` usa — e o
 * `OfflineAudioContext` que decodifica, em `Contexto.ForaDoTempo`.
 *
 * Ele anota o que o `<audio>` de mentira anotava — tocou, parou, soltou —,
 * agora nas peças do WebAudio: a fonte que começa (e de onde) e para, e o
 * ganho que se liga e se desliga da saída. `Contexto.ultimo` é a instância que
 * a região criou, para a prova olhar dentro dela, e `Contexto.criados` conta
 * quantas nasceram: um contexto de tempo real segura a saída do sistema
 * aberta mesmo calado, e quando ele nasce é o que a prova do repouso confere.
 * `suspensoes` e `fechamentos` contam as duas maneiras de soltá-la; a
 * suspensão só muda `state` quando a promessa dela resolve, como no navegador.
 *
 * **Quem decodifica é o contexto fora do tempo**, que não abre a saída: o de
 * tempo real também sabe decodificar, como o de verdade, mas anota cada vez
 * em `Contexto.decodificadosNoTempoReal` — um som montado e parado não tem
 * por que acordar o áudio do sistema. `Contexto.decodificados` junta o que os
 * dois decodificaram, e `Contexto.ForaDoTempo.ultimo.argumentos` é com que o
 * produto o construiu.
 *
 * As opções são os jeitos de o navegador dizer não:
 * - `semGesto`: o áudio da janela nasce parado e `resume()` fica pendurado,
 *   que é o que o Chromium faz com quem pede para tocar antes de um clique;
 * - `recusaNaHora`: o áudio nasce parado e `resume()` recusa na hora, sem
 *   ligá-lo — o desfecho que não depende do prazo;
 * - `fonteLanca`: `createBufferSource` lança, e `tocar()` rejeita em vez de
 *   avisar — o caminho defensivo do `catch` (P13 da varredura do Plano 1D);
 * - `naoDecodifica`: `decodeAudioData` recusa os bytes.
 *
 * E duas que não são recusa:
 * - `decodificado` é o som que `decodeAudioData` devolve — por padrão um
 *   segundo estéreo a 48 kHz. Com `length` e `numberOfChannels`, ele diz
 *   quanto o som ocupa decodificado, que é o que o teto do som decodificado
 *   confere;
 * - `segurarADecodificacao`: a decodificação não responde sozinha, e fica em
 *   `Contexto.decodificacoesPresas` (`{ pronto, falhou }`) para a prova soltar
 *   quando quiser — é como se sai **no meio** dela.
 *
 * @param {object} opcoes `{ semGesto, recusaNaHora, fonteLanca, naoDecodifica, decodificado, segurarADecodificacao }`.
 */
function audioDeMentira(opcoes = {}) {
  function decodificar(bytes, pronto, falhou) {
    Contexto.decodificados.push(bytes.byteLength);
    if (opcoes.segurarADecodificacao) Contexto.decodificacoesPresas.push({ pronto, falhou });
    else if (opcoes.naoDecodifica) falhou(new Error("EncodingError: formato que este motor não conhece"));
    else pronto(opcoes.decodificado ?? { duration: 1, length: 48000, numberOfChannels: 2, sampleRate: 48000 });
    return undefined;
  }
  class Contexto {
    constructor() {
      Contexto.ultimo = this;
      Contexto.criados += 1;
      this.state = opcoes.semGesto || opcoes.recusaNaHora ? "suspended" : "running";
      this.currentTime = 0;
      this.destination = { saida: true };
      this.fontes = [];
      this.ganhos = [];
      /** Quantas vezes alguém pediu para o áudio da janela ligar. */
      this.acordar = 0;
      /** Quantas vezes o produto soltou a saída sem fechar o contexto. */
      this.suspensoes = 0;
      /** Quantas vezes o produto fechou o contexto. */
      this.fechamentos = 0;
    }
    resume() {
      this.acordar += 1;
      // Sem gesto, a promessa fica pendurada: é o que o navegador faz com quem
      // pede para tocar antes de alguém apertar alguma coisa.
      if (opcoes.semGesto) return new Promise(() => {});
      if (opcoes.recusaNaHora) return Promise.reject(new Error("NotAllowedError: o áudio da janela não liga"));
      this.state = "running";
      return Promise.resolve();
    }
    suspend() {
      this.suspensoes += 1;
      // Como o de verdade: `state` só muda quando a promessa resolve, e até
      // lá o contexto diz «running» a quem perguntar.
      return Promise.resolve().then(() => {
        if (this.state !== "closed") this.state = "suspended";
      });
    }
    close() {
      this.fechamentos += 1;
      this.state = "closed";
      return Promise.resolve();
    }
    createGain() {
      const ganho = {
        ligado: null,
        connect(alvo) { this.ligado = alvo; },
        disconnect() { this.ligado = null; },
      };
      this.ganhos.push(ganho);
      return ganho;
    }
    createBufferSource() {
      if (opcoes.fonteLanca) throw new Error("InvalidStateError: a fonte não abriu");
      const fonte = {
        buffer: null,
        ligado: null,
        tocando: false,
        parou: false,
        /** De que ponto do som ela começou, em segundos. */
        de: null,
        onended: null,
        connect(alvo) { this.ligado = alvo; },
        disconnect() { this.ligado = null; },
        start(_quando = 0, de = 0) { this.tocando = true; this.de = de; },
        stop() { this.tocando = false; this.parou = true; },
      };
      this.fontes.push(fonte);
      return fonte;
    }
    decodeAudioData(bytes, pronto, falhou) {
      Contexto.decodificadosNoTempoReal.push(bytes.byteLength);
      return decodificar(bytes, pronto, falhou);
    }
  }
  Contexto.criados = 0;
  Contexto.decodificados = [];
  Contexto.decodificadosNoTempoReal = [];
  Contexto.decodificacoesPresas = [];
  /** O `OfflineAudioContext`: decodifica e não abre saída nenhuma. */
  class ContextoForaDoTempo {
    constructor(...argumentos) {
      ContextoForaDoTempo.ultimo = this;
      ContextoForaDoTempo.criados += 1;
      this.argumentos = argumentos;
    }
    decodeAudioData(bytes, pronto, falhou) {
      return decodificar(bytes, pronto, falhou);
    }
  }
  ContextoForaDoTempo.criados = 0;
  Contexto.ForaDoTempo = ContextoForaDoTempo;
  return Contexto;
}

/**
 * Um relógio que só anda quando a prova manda.
 *
 * O prazo de silêncio que suspende o áudio da janela é de segundos, e esperar
 * por ele de verdade faria a bancada levar esse tempo a cada volta. Aqui o
 * prazo é anotado, e `passar(ms)` roda o que foi marcado para até `ms` — contado
 * de quando foi marcado, que nesta bancada é sempre logo antes.
 */
function relogioDeMentira() {
  const marcados = new Map();
  let serie = 0;
  return {
    setTimeout: (fn, ms = 0) => {
      serie += 1;
      marcados.set(serie, { fn, ms });
      return serie;
    },
    clearTimeout: (id) => {
      marcados.delete(id);
    },
    passar(ms) {
      for (const [id, marcado] of Array.from(marcados)) {
        if (marcado.ms > ms) continue;
        marcados.delete(id);
        marcado.fn();
      }
    },
  };
}

/**
 * Um contexto com o DOM mínimo e `mods-regiao.js` dentro.
 *
 * @param {object} opcoes `{ audio, relogio }`: o WebAudio de mentira desta
 *   prova (`audioDeMentira`), e um relógio que só anda quando ela manda
 *   (`relogioDeMentira`) — sem ele, o de verdade.
 */
function bancada(opcoes = {}) {
  const quadros = [];
  const doc = { activeElement: null };
  doc.createElement = (tag) => new Elemento(tag, doc);
  doc.createTextNode = (data) => new NoDeTexto(data);
  // O documento: as raízes nascem nele, como a da faixa e a de uma página
  // aberta. Uma prova tira uma do documento — `raiz.remove()` — para medir o
  // que sai da tela sem ser descartado.
  doc.body = doc.createElement("body");
  // O WebAudio desta prova, ou o padrão — um que decodifica e toca.
  const Audio = opcoes.audio ?? audioDeMentira();

  const contexto = vm.createContext({
    console,
    document: doc,
    Node: { TEXT_NODE: TEXTO, ELEMENT_NODE: ELEMENTO },
    // O relógio e o `atob` de verdade, salvo quando a prova traz o dela: o
    // tocador espera a política de áudio com prazo, o áudio calado é suspenso
    // depois de outro, e o som que vem do servidor do MOD chega em base64.
    setTimeout: opcoes.relogio?.setTimeout ?? setTimeout,
    clearTimeout: opcoes.relogio?.clearTimeout ?? clearTimeout,
    atob,
    AudioContext: Audio,
    OfflineAudioContext: Audio.ForaDoTempo,
    requestAnimationFrame: (fn) => {
      quadros.push(fn);
      return quadros.length;
    },
    cancelAnimationFrame: (id) => {
      if (id) quadros[id - 1] = null;
    },
    getComputedStyle: () => ({ getPropertyValue: () => "#ffffff" }),
    // O construtor da casa, igual ao de `base.js`: é por ele que a região
    // monta, e um diferente aqui mediria outro código.
    elemento: (tag, classe, texto) => {
      const no = doc.createElement(tag);
      if (classe) no.className = classe;
      if (texto !== undefined) no.textContent = texto;
      return no;
    },
  });
  // `class` e `const` no topo são ligações léxicas: elas não viram
  // propriedades do contexto, e por isso saem por esta linha em vez de por uma
  // leitura de `contexto.RegiaoDeMod`, que devolveria `undefined`.
  vm.runInContext(
    `${fonte}\nglobalThis.api = { RegiaoDeMod, LIMITES_DA_REGIAO, LIMITES_DO_CARTAO, FORMAS_DO_CARTAO, PERFIS_DE_RENDER,
      SILENCIO: typeof SILENCIO_ANTES_DE_SUSPENDER_MS === "number" ? SILENCIO_ANTES_DE_SUSPENDER_MS : undefined,
      encerrarOSomDosMods: typeof encerrarOSomDosMods === "function" ? encerrarOSomDosMods : undefined };`,
    contexto,
    { filename: "mods-regiao.js" },
  );
  return {
    doc,
    quadros,
    RegiaoDeMod: contexto.api.RegiaoDeMod,
    LIMITES: contexto.api.LIMITES_DA_REGIAO,
    CARTAO: contexto.api.LIMITES_DO_CARTAO,
    FORMAS_DO_CARTAO: contexto.api.FORMAS_DO_CARTAO,
    PERFIS: contexto.api.PERFIS_DE_RENDER,
    /** Quanto o áudio calado espera antes de ser suspenso — ver `SILENCIO_ANTES_DE_SUSPENDER_MS`. */
    SILENCIO: contexto.api.SILENCIO,
    /** O que a saída da sessão chama para calar e soltar o som dos MODs. */
    encerrarOSomDosMods: contexto.api.encerrarOSomDosMods,
    /** Uma raiz no documento, como a da faixa ou a de uma página aberta. */
    raiz: () => {
      const raiz = doc.createElement("section");
      doc.body.append(raiz);
      return raiz;
    },
    /** O `Uint8Array` da janela: o que ele cria passa no `instanceof` dela. */
    Uint8Array: vm.runInContext("Uint8Array", contexto),
    /** Tira o áudio de tempo real da janela, depois de montar: o tocar sem WebAudio. */
    semAudioDeTempoReal: () => {
      contexto.AudioContext = undefined;
    },
  };
}

/** Os doze bytes que o dono de mentira entrega como som: o começo de um WAV. */
const BYTES_DE_SOM = Object.freeze([82, 73, 70, 70, 0, 0, 0, 0, 87, 65, 86, 69]);

/** Um dono que anota o que a região fala, o que ela pede, o que ela diz ao registro e quando uma mídia muda de estado. */
function dono(b, midia) {
  const ditos = [];
  const registrados = [];
  const anotadas = [];
  let mudancas = 0;
  const pedidosDeSom = [];
  return {
    ditos,
    registrados,
    anotadas,
    pedidosDeSom,
    /** Quantas vezes a região avisou que o estado de uma mídia mudou. */
    mudancas: () => mudancas,
    api: {
      instancia: {
        registrar: (porque, descartar) => registrados.push({ porque, descartar }),
      },
      geracao: 1,
      podeFalar: () => true,
      falar: (dados) => ditos.push(dados),
      anotarRecusa: (texto) => anotadas.push(String(texto)),
      carregarMidia: (caminho) =>
        midia ? midia(caminho) : Promise.reject(new Error("sem mídia")),
      carregarMidiaDoServidor: (canal, pedido, campo) =>
        midia ? midia({ canal, pedido, campo }) : Promise.reject(new Error("sem mídia")),
      // Os bytes de um som do pacote, como `som_do_mod` os devolve pelo
      // protocolo `ipc:`, que é o caminho normal: um `ArrayBuffer` (a lista de
      // números do recuo por `postMessage` tem prova própria, em
      // `osBytesDoSomChegamPelosDoisCaminhosDoIpc`). **Do reino da janela**:
      // um criado aqui fora, no do node, não passa no `instanceof ArrayBuffer`
      // de `bufferDoSom`, e a prova mediria outro ramo (I-4 da revisão ampla do
      // Plano 1D). O conteúdo não importa — quem decodifica é o WebAudio de
      // mentira —, e o pedido anotado diz qual caminho foi pedido.
      bytesDoSom: (caminho) => {
        pedidosDeSom.push(caminho);
        return Promise.resolve(new b.Uint8Array(BYTES_DE_SOM).buffer);
      },
      midiaMudou: () => {
        mudancas += 1;
      },
    },
  };
}

// ---------------------------------------------------------------------------
// Os cartões: mesma gramática, tetos próprios, e saem com a região.
// ---------------------------------------------------------------------------

/** Acha o primeiro descendente com uma etiqueta, para não depender de posição. */
function acharTag(no, tag) {
  if (!no || !no.filhos) return null;
  const alvo = String(tag).toUpperCase();
  for (const filho of no.filhos) {
    if (filho.tagName === alvo) return filho;
    const dentro = acharTag(filho, tag);
    if (dentro) return dentro;
  }
  return null;
}

/** Todos os descendentes com uma etiqueta, na ordem do documento. */
function todasAsTags(no, tag) {
  const alvo = String(tag).toUpperCase();
  const achadas = [];
  for (const filho of no?.filhos ?? []) {
    if (filho.tagName === alvo) achadas.push(filho);
    achadas.push(...todasAsTags(filho, tag));
  }
  return achadas;
}

/** O texto que um nó carrega, juntando os nós de texto de dentro dele. */
function textoDe(no) {
  if (!no) return "";
  if (no.nodeType === TEXTO) return no.data;
  return (no.filhos ?? []).map(textoDe).join("");
}

function contarNos(no) {
  if (!no || !no.filhos) return 0;
  return no.filhos.reduce((soma, filho) => soma + 1 + contarNos(filho), 0);
}

async function oCartaoUsaOMesmoRendererComGramaticaMenor() {
  const b = bancada();
  const d = dono(b);
  const regiao = new b.RegiaoDeMod("seele/perfis", d.api, b.raiz());

  const recusados = regiao.declararCartoes({
    7: [
      { forma: "titulo", dentro: "Lia" },
      { forma: "texto", dentro: "ela/dela" },
      // As três que um cartão não aceita. Elas não somem caladas: contam como
      // recusa, porque o MOD precisa saber que pôs botão onde botão não entra.
      { forma: "botao", chave: "b", dentro: "APERTE" },
      { forma: "campo", chave: "c", rotulo: "R", valor: "v" },
      { forma: "tela", chave: "t", largura: 10, altura: 10 },
    ],
  });

  const cartao = regiao.cartaoDe(7);
  confere("o cartão existe", Boolean(cartao), "não foi montado");
  confere("o cartão é do produto", cartao?.className === "pessoa-cartao", String(cartao?.className));

  // **Montado pelo renderer, e não por HTML.** As etiquetas são as que
  // `FORMAS_DA_REGIAO` dita, e o texto entrou por `textContent`.
  confere("o título virou h3", Boolean(acharTag(cartao, "h3")), "sem h3");
  confere("o texto virou p", Boolean(acharTag(cartao, "p")), "sem p");
  confere(
    "o texto é o que o MOD disse",
    textoDe(acharTag(cartao, "h3")) === "Lia",
    textoDe(acharTag(cartao, "h3")),
  );

  for (const proibida of ["button", "label", "canvas"]) {
    confere(
      `o cartão não aceita ${proibida}`,
      acharTag(cartao, proibida) === null,
      `um ${proibida} entrou no cartão`,
    );
  }
  confere("as três recusas foram contadas", recusados === 3, `contou ${recusados}`);

  // E a gramática do cartão é um subconjunto declarado, e não uma lista solta.
  for (const fora of ["campo", "escolha", "botao", "arquivo", "tela"]) {
    confere(
      `${fora} está fora da gramática do cartão`,
      !b.FORMAS_DO_CARTAO.has(fora),
      `${fora} está em FORMAS_DO_CARTAO`,
    );
  }
}

async function oCartaoSaiQuandoOModParaDeDeclararOuQuandoAReGiaoSai() {
  const b = bancada();
  const d = dono(b, () => Promise.resolve({ uri: "x:", papel: "imagem", bytes: 1000 }));
  const regiao = new b.RegiaoDeMod("seele/perfis", d.api, b.raiz());

  regiao.declararCartoes({
    7: [{ forma: "midia", chave: "retrato", doServidor: { canal: 1, pedido: {}, campo: "image" } }],
    9: [{ forma: "texto", dentro: "outro" }],
  });
  await volta();
  await volta();

  const soltos = () => d.registrados.filter((r) => r.porque.includes("midia")).length;
  confere("o retrato foi registrado", soltos() === 1, `${soltos()} registros`);
  confere("os bytes entraram na conta do cartão", regiao.bytesDeCartao === 1000, String(regiao.bytesDeCartao));
  confere("e não na conta da região", regiao.bytesDeMidia === 0, String(regiao.bytesDeMidia));

  // Parar de declarar uma pessoa tira o cartão dela e solta o que ele segurava.
  regiao.declararCartoes({ 9: [{ forma: "texto", dentro: "outro" }] });
  confere("o cartão de 7 saiu", regiao.cartaoDe(7) === null, "continuou lá");
  confere("o de 9 ficou", regiao.cartaoDe(9) !== null, "sumiu junto");
  confere("os bytes voltaram", regiao.bytesDeCartao === 0, String(regiao.bytesDeCartao));
  confere("a contagem voltou", regiao.contagem.midiasDeCartao === 0, String(regiao.contagem.midiasDeCartao));

  // E soltar a região tira o resto, **mesmo o que nunca esteve sob a raiz
  // dela**. Um cartão mora na lista do produto, e não na região: pendurá-lo
  // aqui é o que faz esta prova medir alguma coisa.
  //
  // A primeira versão disto media nada: `cartaoDe` devolve `null` assim que a
  // região está solta, e a raiz nunca fora pendurada em lugar nenhum — as duas
  // asserções passavam com o descarte **removido**. A reversão pegou.
  regiao.declararCartoes({
    9: [{ forma: "midia", chave: "retrato", doServidor: { canal: 1, pedido: {}, campo: "image" } }],
  });
  await volta();
  await volta();
  const lista = b.doc.createElement("li");
  const noDoNove = regiao.cartaoDe(9);
  lista.append(noDoNove);
  confere("o cartão está na lista", noDoNove.pai === lista, "não foi pendurado");
  confere("e segura o retrato", regiao.bytesDeCartao === 1000, String(regiao.bytesDeCartao));

  regiao.soltar();
  confere("o cartão saiu da lista do produto", noDoNove.pai === null, "continuou pendurado ao lado do nome");
  confere("e a região esqueceu a raiz", regiao.raizesDeCartao.size === 0, `${regiao.raizesDeCartao.size} raiz(es)`);
  confere("e o retrato foi solto", regiao.bytesDeCartao === 0, String(regiao.bytesDeCartao));
}

async function oRetratoDoCartaoEOMesmoNoEntreDoisRetratos() {
  const b = bancada();
  const d = dono(b, () => Promise.resolve({ uri: "x:", papel: "imagem", bytes: 10 }));
  const regiao = new b.RegiaoDeMod("seele/perfis", d.api, b.raiz());

  const declarar = (nome) =>
    regiao.declararCartoes({
      7: [
        { forma: "midia", chave: "retrato", doServidor: { canal: 1, pedido: {}, campo: "image" } },
        { forma: "texto", chave: "nome", dentro: nome },
      ],
    });

  declarar("Lia");
  await volta();
  await volta();
  const antes = acharTag(regiao.cartaoDe(7), "img");
  confere("o retrato montou", Boolean(antes), "sem img");

  // **O mesmo nó.** Um `<img>` recriado a cada quatro segundos pisca, e um
  // `<audio>` recriado recomeça — é o motivo de a lista receber o nó montado
  // em vez de uma cópia, e de a reconciliação valer aqui como vale na região.
  declarar("Lia Nova");
  const depois = acharTag(regiao.cartaoDe(7), "img");
  confere("o retrato não foi recriado", antes === depois, "o nó da imagem trocou");
  confere("e o texto mudou", textoDe(regiao.cartaoDe(7)).includes("Lia Nova"), textoDe(regiao.cartaoDe(7)));
  confere("os bytes não dobraram", regiao.bytesDeCartao === 10, String(regiao.bytesDeCartao));
}

async function aImagemAcompanhaAMudancaDaFonte() {
  for (const forma of ["retrato", "midia"]) {
    const b = bancada();
    const pendentes = [];
    const d = dono(b, () => new Promise(resolve => pendentes.push(resolve)));
    const regiao = new b.RegiaoDeMod("seele/perfis", d.api, b.raiz());
    const declarar = versao => regiao.aplicar([{
      forma, chave: "foto", inicial: "A",
      ...(versao ? { doServidor: { canal: 1, pedido: { op: "asset", path: versao }, campo: "image" } } : {}),
    }]);
    declarar(null);
    declarar("primeira");
    confere(forma + " ganha fonte", pendentes.length === 1, "a seleção não iniciou leitura");
    if (pendentes.length !== 1) continue;
    declarar("segunda");
    confere(forma + " troca fonte", pendentes.length === 2, "a troca não iniciou leitura");
    if (pendentes.length !== 2) continue;
    pendentes[0]({ uri: "velha:", papel: "imagem", bytes: 10 });
    pendentes[1]({ uri: "nova:", papel: "imagem", bytes: 20 });
    await volta(); await volta();
    const imagem = acharTag(regiao.raiz, "img");
    confere(forma + " ignora atrasada", imagem?.src === "nova:" && regiao.bytesDeMidia === 20, "montou resposta antiga ou contou duas vezes");
    declarar("segunda");
    confere(forma + " conserva a fonte", acharTag(regiao.raiz, "img") === imagem && pendentes.length === 2, "recarregou sem mudança");
    declarar(null);
    confere(forma + " remove fonte", !acharTag(regiao.raiz, "img") && regiao.bytesDeMidia === 0, "imagem removida continua visível");
    regiao.soltar();
  }
}

async function osTetosDoCartaoSaoDoCartaoENaoDaRegiao() {
  const b = bancada();
  const d = dono(b);
  const regiao = new b.RegiaoDeMod("seele/perfis", d.api, b.raiz());

  // Pessoas demais é recusa **inteira**, e com o teto escrito: o MOD pediu um
  // conjunto, e um conjunto pela metade é pior que nenhum.
  const demais = {};
  for (let i = 0; i <= b.CARTAO.cartoes; i += 1) demais[i] = [{ forma: "texto", dentro: "a" }];
  let recusou = "";
  try {
    regiao.declararCartoes(demais);
  } catch (erro) {
    recusou = String(erro.message);
  }
  confere("pessoas demais é recusa", recusou.includes(String(b.CARTAO.cartoes)), recusou || "aceitou");
  confere("e nada ficou pela metade", regiao.cartaoDe(0) === null, "gravou parte do conjunto");

  // O teto de nós é o do cartão, e não o da região: vinte e quatro, e não 512.
  const muitos = [];
  for (let i = 0; i < b.CARTAO.nos + 30; i += 1) muitos.push({ forma: "texto", dentro: `n${i}` });
  const recusados = regiao.declararCartoes({ 3: muitos });
  const quantos = contarNos(regiao.cartaoDe(3));
  confere("o cartão parou no teto dele", quantos <= b.CARTAO.nos, `montou ${quantos}`);
  confere("e a recusa foi contada", recusados > 0, "recusou calado");
  confere(
    "o teto do cartão é menor que o da região",
    b.CARTAO.nos < b.LIMITES.nos,
    `cartão ${b.CARTAO.nos} · região ${b.LIMITES.nos}`,
  );

  // Um cartão vazio não vira moldura vazia ao lado de um nome.
  regiao.declararCartoes({ 5: [] });
  confere("cartão vazio não entra", regiao.cartaoDe(5) === null, "montou uma moldura vazia");
}

// ---------------------------------------------------------------------------
// 1. Atualizar com um campo em foco não tira o campo do documento.
// ---------------------------------------------------------------------------

async function oFocoSobreviveAAtualizacao() {
  const caso = "o foco sobrevive à atualização";
  const b = bancada();
  const d = dono(b);
  const raiz = b.raiz();
  const regiao = new b.RegiaoDeMod("a/b", d.api, raiz);

  const declarar = (valor, contador) => [
    { forma: "titulo", chave: "t", dentro: `contagem ${contador}` },
    { forma: "campo", chave: "nome", rotulo: "NOME", valor },
  ];

  regiao.aplicar(declarar("", 0));
  const campo = raiz.children[1];
  const caixa = campo.querySelector(".regiao-de-mod-caixa");
  confere(caso, !!caixa, "a caixa não foi montada");
  if (!caixa) return;

  // A pessoa põe o foco e digita.
  b.doc.activeElement = caixa;
  caixa.value = "ale";
  caixa.disparar("input");
  confere(
    caso,
    d.ditos.some((e) => e.nome === "campo" && e.valor === "ale"),
    `o que foi digitado não chegou ao MOD: ${JSON.stringify(d.ditos)}`,
  );

  // E o MOD responde redesenhando — ecoando um valor antigo, que é o que um
  // MOD que grava no servidor faz enquanto a gravação não volta.
  const saidasAntes = caixa.saiuDoDocumento;
  regiao.aplicar(declarar("", 1));

  confere(
    caso,
    caixa.saiuDoDocumento === saidasAntes,
    `a caixa saiu do documento na atualização (${caixa.saiuDoDocumento - saidasAntes}x), ` +
      "e sair é perder o foco",
  );
  confere(caso, b.doc.activeElement === caixa, "o foco foi perdido");
  confere(
    caso,
    caixa.value === "ale",
    `o valor de quem estava digitando foi reescrito para «${caixa.value}»`,
  );
  // E o título, que não tem foco, foi atualizado do mesmo jeito.
  confere(
    caso,
    raiz.children[0].filhos[0]?.data === "contagem 1",
    "o texto vizinho não acompanhou a atualização",
  );

  // Sem foco, o valor que o MOD declara volta a mandar.
  b.doc.activeElement = null;
  regiao.aplicar(declarar("gravado", 2));
  confere(
    caso,
    caixa.value === "gravado",
    `sem foco, o valor declarado não foi aplicado: «${caixa.value}»`,
  );
  confere(
    caso,
    caixa.saiuDoDocumento === saidasAntes,
    "a caixa foi refeita quando só o valor mudou",
  );
}

// ---------------------------------------------------------------------------
// 2. O arraste vira traço, agregado por quadro.
// ---------------------------------------------------------------------------

async function oArrasteViraTracoAgregado() {
  const caso = "o arraste vira traço agregado";
  const b = bancada();
  const d = dono(b);
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar([{ forma: "tela", chave: "t", largura: 100, altura: 100 }]);
  const tela = regiao.raiz.children[0];
  tela.pincel = {
    clearRect() {},
    beginPath() {},
    moveTo() {},
    lineTo() {},
    stroke() {
      this.tracos = (this.tracos ?? 0) + 1;
    },
  };

  tela.disparar("pointerdown", { clientX: 10, clientY: 10, pointerId: 1 });
  for (let i = 0; i < 50; i++) {
    tela.disparar("pointermove", { clientX: 10 + i, clientY: 20, pointerId: 1 });
  }
  confere(
    caso,
    d.ditos.filter((e) => e.fase === "moveu").length === 0,
    "o movimento foi mandado antes do quadro, e um arraste satura a fila assim",
  );
  // O quadro vem, e **um** movimento sai — o último.
  b.quadros.shift()?.();
  const movidos = d.ditos.filter((e) => e.fase === "moveu");
  confere(
    caso,
    movidos.length === 1,
    `cinquenta movimentos viraram ${movidos.length} mensagens em vez de uma`,
  );
  confere(
    caso,
    movidos[0]?.x === 59,
    `a mensagem não levou o último ponto: ${JSON.stringify(movidos[0])}`,
  );

  tela.disparar("pointerup", { clientX: 60, clientY: 20, pointerId: 1 });
  confere(
    caso,
    d.ditos.at(-1)?.fase === "terminou",
    "o fim do arraste não foi dito",
  );

  // E o MOD declara o traço de volta, que é quem pinta.
  regiao.aplicar([
    {
      forma: "tela",
      chave: "t",
      largura: 100,
      altura: 100,
      tracos: [[{ x: 10, y: 10 }, { x: 59, y: 20 }]],
    },
  ]);
  confere(caso, tela.pincel.tracos === 1, "o traço declarado não foi pintado");
}

// ---------------------------------------------------------------------------
// 2b. Um tabuleiro: figuras declaradas, e o arraste diz qual foi pega.
// ---------------------------------------------------------------------------

async function oArrastePegaAFiguraDeCima() {
  const caso = "o arraste pega a figura de cima";
  const b = bancada();
  const d = dono(b);
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());

  // Duas peças **sobrepostas**, e uma parede por baixo de tudo. A de cima é a
  // última declarada, e é ela que o dedo tem de encontrar.
  const tabuleiro = (ondeX) => [{
    forma: "tela",
    chave: "mapa",
    largura: 200,
    altura: 200,
    figuras: [
      { tipo: "linha", chave: "parede", x: 0, y: 0, ate_x: 200, ate_y: 200 },
      { tipo: "retangulo", chave: "peca-de-baixo", x: 40, y: 40, largura: 40, altura: 40, cor: "#334455" },
      { tipo: "circulo", chave: "peca-de-cima", x: ondeX, y: 60, raio: 15, cor: "#6bffb6" },
      { tipo: "texto", chave: "nome", x: 10, y: 170, dentro: "GOBLIN", corpo: 12 },
    ],
  }];

  regiao.aplicar(tabuleiro(60));
  const tela = regiao.raiz.children[0];
  tela.darPincel();
  regiao.aplicar(tabuleiro(60));

  confere(
    caso,
    tela.pincel.pintado.some((p) => p[0] === "retangulo") &&
      tela.pincel.pintado.includes("circulo") &&
      tela.pincel.pintado.some((p) => p[0] === "texto" && p[1] === "GOBLIN"),
    `as figuras declaradas não foram pintadas: ${JSON.stringify(tela.pincel.pintado)}`,
  );

  // O dedo desce onde as duas peças se sobrepõem.
  tela.disparar("pointerdown", { clientX: 60, clientY: 60, pointerId: 1 });
  const comecou = d.ditos.at(-1);
  confere(
    caso,
    comecou?.alvo === "peca-de-cima",
    `pegou «${comecou?.alvo}» em vez da peça de cima`,
  );

  // E o alvo **viaja** nas três fases: o dedo sai de cima da peça e ela
  // continua sendo a que está sendo arrastada.
  tela.disparar("pointermove", { clientX: 150, clientY: 150, pointerId: 1 });
  b.quadros.shift()?.();
  const moveu = d.ditos.at(-1);
  confere(
    caso,
    moveu?.fase === "moveu" && moveu?.alvo === "peca-de-cima",
    `o alvo se perdeu no meio do arraste: ${JSON.stringify(moveu)}`,
  );
  tela.disparar("pointerup", { clientX: 150, clientY: 150, pointerId: 1 });
  const fimDoArraste = d.ditos.at(-1);
  confere(
    caso,
    fimDoArraste?.fase === "terminou" && fimDoArraste?.alvo === "peca-de-cima" && fimDoArraste?.x === 150,
    `o fim do arraste não levou peça e destino: ${JSON.stringify(fimDoArraste)}`,
  );

  // O MOD move a peça redeclarando-a, e o acerto acompanha.
  regiao.aplicar(tabuleiro(150));
  tela.disparar("pointerdown", { clientX: 60, clientY: 60, pointerId: 2 });
  confere(
    caso,
    d.ditos.at(-1)?.alvo === "peca-de-baixo",
    `depois de a peça sair dali, o toque ainda a encontra: ${d.ditos.at(-1)?.alvo}`,
  );

  // Um toque no vazio responde «nada», e não some com o campo.
  tela.disparar("pointerup", { clientX: 60, clientY: 60, pointerId: 2 });
  tela.disparar("pointerdown", { clientX: 5, clientY: 5, pointerId: 3 });
  const vazio = d.ditos.at(-1);
  confere(
    caso,
    "alvo" in vazio && vazio.alvo === null,
    `um toque no vazio não disse «nada»: ${JSON.stringify(vazio)}`,
  );

  // Uma parede não é pega: ela é grade, e pegá-la roubaria o toque das peças.
  tela.disparar("pointerup", { clientX: 5, clientY: 5, pointerId: 3 });
  tela.disparar("pointerdown", { clientX: 100, clientY: 100, pointerId: 4 });
  confere(
    caso,
    d.ditos.at(-1)?.alvo === null,
    `a linha foi pega, e ela é parede: ${d.ditos.at(-1)?.alvo}`,
  );
}

// ---------------------------------------------------------------------------
// 3. Sair durante o carregamento não monta mídia.
// ---------------------------------------------------------------------------

async function sairDuranteOCarregamentoNaoMonta() {
  const caso = "sair durante o carregamento";
  const b = bancada();
  let soltar;
  const carregando = new Promise((r) => {
    soltar = r;
  });
  const d = dono(b, () => carregando);
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar([{ forma: "midia", chave: "m", fonte: "som/a.wav" }]);
  const figura = regiao.raiz.children[0];
  confere(
    caso,
    figura.dataset.estado === "carregando",
    `o estado do carregamento não foi dito: ${figura.dataset.estado}`,
  );

  // A pessoa sai **enquanto** os bytes vêm.
  regiao.soltar();
  soltar({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 2 });
  await volta();
  await volta();

  confere(
    caso,
    figura.querySelector(".regiao-de-mod-tocador") === null,
    "a mídia foi montada depois de a região ter sido solta",
  );
  confere(caso, regiao.contagem.midias === 0, `a conta de mídias ficou em ${regiao.contagem.midias}`);
  confere(caso, regiao.bytesDeMidia === 0, `sobraram ${regiao.bytesDeMidia} bytes contados`);
}

// ---------------------------------------------------------------------------
// 3b. A mídia do servidor: outra origem, o mesmo dono e o mesmo descarte.
// ---------------------------------------------------------------------------

async function aMidiaDoServidorTemOMesmoDono() {
  const caso = "a mídia do servidor";
  const b = bancada();
  const pedidos = [];
  let soltar;
  const carregando = new Promise((r) => { soltar = r; });
  const d = dono(b, (o_que) => {
    pedidos.push(o_que);
    return carregando;
  });
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());

  regiao.aplicar([
    {
      forma: "midia",
      chave: "cena",
      doServidor: { canal: 7, pedido: { op: "scene-image", id: 3 } },
    },
  ]);
  confere(
    caso,
    pedidos.length === 1 && pedidos[0].canal === 7 && pedidos[0].pedido?.op === "scene-image",
    `o pedido não foi ao servidor com canal e operação: ${JSON.stringify(pedidos)}`,
  );

  // A pessoa sai **enquanto** os bytes vêm — o mesmo caso da mídia de pacote,
  // e ele tem de valer para as duas origens.
  regiao.soltar();
  soltar({ uri: "data:image/png;base64,AA", papel: "imagem", bytes: 900 });
  await volta();
  await volta();
  confere(
    caso,
    regiao.raiz.children[0]?.querySelector(".regiao-de-mod-tocador") == null,
    "a mídia do servidor foi montada depois de a região ter sido solta",
  );
  confere(caso, regiao.bytesDeMidia === 0, `sobraram ${regiao.bytesDeMidia} bytes contados`);

  // E, sem sair, ela monta e é descartada como a outra.
  const b2 = bancada();
  const d2 = dono(b2, () =>
    Promise.resolve({ uri: "data:image/png;base64,AA", papel: "imagem", bytes: 900 }),
  );
  const regiao2 = new b2.RegiaoDeMod("a/b", d2.api, b2.raiz());
  regiao2.aplicar([{ forma: "midia", chave: "cena", doServidor: { canal: 1, pedido: {} } }]);
  await volta();
  await volta();
  const tocador = regiao2.raiz.children[0].querySelector(".regiao-de-mod-tocador");
  confere(caso, tocador?.tagName === "IMG", `o papel «imagem» não virou <img>: ${tocador?.tagName}`);
  confere(caso, regiao2.bytesDeMidia === 900, `os bytes não foram contados: ${regiao2.bytesDeMidia}`);
  regiao2.soltar();
  confere(caso, regiao2.bytesDeMidia === 0, "o descarte não devolveu os bytes da mídia do servidor");

  // Sem nenhuma das duas origens, o estado é dito em vez de a mídia sumir.
  const b3 = bancada();
  const regiao3 = new b3.RegiaoDeMod("a/b", dono(b3).api, b3.raiz());
  regiao3.aplicar([{ forma: "midia", chave: "nada" }]);
  confere(
    caso,
    regiao3.raiz.children[0]?.dataset.estado === "sem-fonte",
    `uma mídia sem origem não disse o estado dela: ${regiao3.raiz.children[0]?.dataset.estado}`,
  );
}

// ---------------------------------------------------------------------------
// 4. Sair durante a reprodução para o som e solta os bytes.
// ---------------------------------------------------------------------------

async function sairDuranteAReproducaoPara() {
  const caso = "sair durante a reprodução";
  const Audio = audioDeMentira();
  const b = bancada({ audio: Audio });
  const d = dono(b, () =>
    Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 1024 }),
  );
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar([{ forma: "midia", chave: "m", fonte: "som/a.wav", tocando: true }]);
  await assentar();

  const contexto = Audio.ultimo;
  const fonte = contexto?.fontes[0];
  confere(caso, !!fonte, "o som declarado não chegou a ter fonte de áudio");
  if (!fonte) return;
  confere(caso, fonte.tocando === true, "o `tocando` declarado não tocou");
  confere(caso, regiao.bytesDeMidia === 1024, `os bytes não foram contados: ${regiao.bytesDeMidia}`);

  // A pessoa sai no meio do som.
  regiao.soltar();
  confere(caso, fonte.parou === true, "o som continuou tocando depois da saída");
  confere(
    caso,
    contexto.ganhos.every((ganho) => ganho.ligado === null),
    "o ganho continuou ligado à saída, e o som decodificado fica preso a ela",
  );
  confere(caso, regiao.bytesDeMidia === 0, `sobraram ${regiao.bytesDeMidia} bytes contados`);
  confere(
    caso,
    regiao.raiz.ouvintesDePe() === 0,
    `sobraram ${regiao.raiz.ouvintesDePe()} ouvintes presos depois da saída`,
  );
}

// ---------------------------------------------------------------------------
// 5. Tirar um nó da declaração solta o que ele segurava.
// ---------------------------------------------------------------------------

async function tirarUmNoSoltaOQueEleSegurava() {
  const caso = "tirar um nó solta o recurso dele";
  const Audio = audioDeMentira();
  const b = bancada({ audio: Audio });
  const d = dono(b, () =>
    Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 512 }),
  );
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar([
    { forma: "midia", chave: "m", fonte: "som/a.wav", tocando: true },
    { forma: "campo", chave: "c", rotulo: "X", valor: "" },
  ]);
  await assentar();
  const fonte = Audio.ultimo?.fontes[0];
  confere(caso, fonte?.tocando === true, "o som declarado não começou");
  confere(caso, regiao.contagem.midias === 1 && regiao.contagem.campos === 1, "a conta inicial está errada");

  // O MOD redesenha **sem** a mídia: ela sai da tela, e o som tem de parar.
  regiao.aplicar([{ forma: "campo", chave: "c", rotulo: "X", valor: "" }]);
  confere(caso, fonte?.parou === true, "o som continuou depois de o MOD tirar a mídia da tela");
  confere(caso, regiao.contagem.midias === 0, `a conta de mídias ficou em ${regiao.contagem.midias}`);
  confere(caso, regiao.bytesDeMidia === 0, `sobraram ${regiao.bytesDeMidia} bytes contados`);
  // E o campo, que continua declarado, não foi mexido.
  confere(caso, regiao.contagem.campos === 1, "o campo que continuou declarado foi recriado");
}

// ---------------------------------------------------------------------------
// 6. Os tetos contêm, e a recusa é dita.
// ---------------------------------------------------------------------------

async function osTetosContemEARecusaEDita() {
  const caso = "os tetos contêm e a recusa é dita";
  const b = bancada();
  const d = dono(b);
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());

  // Rasa e larguíssima: cabe na fundura, e é o teto de nós que a segura.
  const larga = Array.from({ length: b.LIMITES.nos + 200 }, (_, i) => ({
    forma: "texto",
    chave: `t${i}`,
  }));
  const recusados = regiao.aplicar(larga);
  confere(caso, recusados > 0, "uma árvore acima do teto não recusou nada");
  confere(
    caso,
    regiao.raiz.children.length <= b.LIMITES.nos,
    `montou ${regiao.raiz.children.length} nós, acima do teto de ${b.LIMITES.nos}`,
  );

  // E o teto por forma: mais campos do que cabem.
  const regiao2 = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  const campos = Array.from({ length: b.LIMITES.campos + 5 }, (_, i) => ({
    forma: "campo",
    chave: `c${i}`,
    rotulo: "X",
  }));
  const recusados2 = regiao2.aplicar(campos);
  confere(caso, recusados2 === 5, `recusou ${recusados2} campos em vez de 5`);
  confere(
    caso,
    regiao2.contagem.campos === b.LIMITES.campos,
    `montou ${regiao2.contagem.campos} campos, acima do teto`,
  );

  // Uma forma que a API não conhece não vira nada.
  const regiao3 = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao3.aplicar([{ forma: "script", dentro: "x" }, { forma: "texto", dentro: "ok" }]);
  confere(
    caso,
    regiao3.raiz.children.length === 1 && regiao3.raiz.children[0].tagName === "P",
    "uma forma desconhecida virou elemento",
  );

  // **As duas recusas por teto dizem a mesma palavra.** A figura, o evento do
  // MOD e o começo da linha do registro, sem a chave: o teto de bytes e o do
  // som decodificado são o mesmo tipo de não, e um MOD que trata «recusada»
  // não pode ter de tratar o segundo como uma falha de carga.
  const comoORecusa = (figura, dito, linha) => ({
    figura: figura?.dataset.estado,
    evento: dito?.estado,
    registro: String(linha ?? "").replace(/«[^»]*»/, "«»").split(":")[0],
  });
  const aRecusaPorTeto = { figura: "cheia", evento: "recusada", registro: "mídia «» recusada" };

  // O teto de bytes: o arquivo como chegou. O MOD recebe `recusada` **com o
  // porquê e os números**, como no teto do som: sem eles, «recusada» por teto
  // e «recusada» por falta de um gesto chegavam iguais.
  {
    const bt = bancada();
    const dt = dono(bt, () => Promise.resolve({ uri: "x:", papel: "imagem", bytes: bt.LIMITES.bytesDeMidia + 1 }));
    const r = new bt.RegiaoDeMod("a/b", dt.api, bt.raiz());
    r.aplicar([{ forma: "midia", chave: "grande", fonte: "img/g.png" }]);
    await assentar();
    const dito = dt.ditos.find((e) => e.nome === "midia" && e.chave === "grande");
    const linha = dt.anotadas.find((t) => t.includes("«grande»"));
    confere(
      caso,
      String(dito?.porque ?? "").includes(String(bt.LIMITES.bytesDeMidia)),
      `o MOD não soube por que a mídia acima do teto de bytes foi recusada, nem com que números: ${JSON.stringify(dt.ditos)}`,
    );
    confere(
      caso,
      JSON.stringify(comoORecusa(r.raiz.children[0], dito, linha)) === JSON.stringify(aRecusaPorTeto),
      `a recusa pelo teto de bytes mudou de palavra: ${JSON.stringify(comoORecusa(r.raiz.children[0], dito, linha))}`,
    );
    r.soltar();
  }

  // **O som decodificado tem teto próprio, somado por bolso.** O teto de bytes
  // conta o arquivo como chegou, comprimido; `decodeAudioData` guarda o som
  // inteiro em float32, na taxa da decodificação, já na montagem — dez MiB de MP3 a
  // 128 kbps viram uns 250 MB, e a 32 kbps passam de 1 GB. Aqui cada som ocupa
  // seis décimos do teto: o primeiro cabe, o segundo é recusado e dito, e não
  // vira tocador nem toca, mesmo declarado `tocando`. É a soma que conta: um
  // teto por som deixaria quatro sons logo abaixo dele valerem quatro tetos.
  const tetoDoSom = b.LIMITES.bytesDeSomDecodificado;
  confere(caso, Number.isFinite(tetoDoSom) && tetoDoSom > 0, `a região não tem teto para o som decodificado: ${tetoDoSom}`);
  if (Number.isFinite(tetoDoSom) && tetoDoSom > 0) {
    // Estéreo, quatro bytes por amostra: o que o WebAudio guarda.
    const quadros = Math.floor((tetoDoSom * 0.6) / 8);
    const Audio = audioDeMentira({ decodificado: { duration: quadros / 48000, length: quadros, numberOfChannels: 2 } });
    const bs = bancada({ audio: Audio });
    const ds = dono(bs, () => Promise.resolve({ uri: "data:audio/ogg;base64,AA", papel: "som", bytes: 12 }));
    const r = new bs.RegiaoDeMod("a/b", ds.api, bs.raiz());
    const som = (chave) => ({ forma: "midia", chave, fonte: `som/${chave}.ogg`, tocando: true });
    r.aplicar([som("s1"), som("s2")]);
    await assentar();
    const [um, dois] = r.raiz.children;
    confere(caso, um?.dataset.estado === "pronta", `o primeiro som, que cabe no teto decodificado, não ficou pronto: ${um?.dataset.estado}`);
    confere(caso, dois?.dataset.estado === "cheia", `o som que passa do teto decodificado não foi recusado: ${dois?.dataset.estado}`);
    confere(
      caso,
      (Audio.ultimo?.ganhos.length ?? 0) === 1 && (Audio.ultimo?.fontes.length ?? 0) === 1,
      `o som acima do teto virou tocador e segura o som decodificado inteiro: ${Audio.ultimo?.ganhos.length} ganho(s), ${Audio.ultimo?.fontes.length} fonte(s)`,
    );
    confere(caso, acharTag(dois, "button") === null, "o som acima do teto decodificado ganhou um botão de tocar");
    const dito = ds.ditos.find((e) => e.nome === "midia" && e.chave === "s2");
    confere(
      caso,
      /decodificado/.test(dito?.porque ?? "") && String(dito?.porque ?? "").includes(String(tetoDoSom)),
      `o MOD não soube que o som passou do teto decodificado, nem com que números: ${JSON.stringify(ds.ditos)}`,
    );
    const linha = ds.anotadas.find((t) => t.includes("«s2»"));
    confere(
      caso,
      /decodificado/.test(linha ?? "") && /teto/.test(linha ?? ""),
      `o som acima do teto decodificado não chegou ao registro: ${JSON.stringify(ds.anotadas)}`,
    );
    confere(
      caso,
      JSON.stringify(comoORecusa(dois, dito, linha)) === JSON.stringify(aRecusaPorTeto),
      "o teto do som decodificado não recusa com a palavra do teto de bytes — "
        + `${JSON.stringify(comoORecusa(dois, dito, linha))}, e o de bytes diz ${JSON.stringify(aRecusaPorTeto)}`,
    );
    confere(
      caso,
      r.midiasAnotadas().filter((a) => a.situacao === "recusada" && /decodificado/.test(a.motivo)).length === 1,
      `o diagnóstico não anotou o som acima do teto decodificado: ${JSON.stringify(r.midiasAnotadas())}`,
    );
    confere(
      caso,
      r.bytesDeSomDecodificado === quadros * 2 * 4,
      `a conta do som decodificado não é a do som que ficou de pé: ${r.bytesDeSomDecodificado}`,
    );
    // Tirar o som devolve o que ele ocupava, e o espaço serve ao próximo.
    r.aplicar([som("s2")]);
    confere(caso, r.bytesDeSomDecodificado === 0, `tirar o som não devolveu o que ele ocupava decodificado: ${r.bytesDeSomDecodificado}`);
    r.aplicar([som("s2"), som("s3")]);
    await assentar();
    confere(
      caso,
      r.raiz.children[1]?.dataset.estado === "pronta",
      `o espaço que o som tirado deixou não voltou para o próximo: ${r.raiz.children[1]?.dataset.estado}`,
    );
    r.soltar();
    confere(caso, r.bytesDeSomDecodificado === 0, `sobraram ${r.bytesDeSomDecodificado} bytes de som decodificado contados depois da saída`);
  }

  // E o bolso dos cartões tem o teto dele: um som de cartão acima do teto não
  // vira tocador nem toca, mesmo declarado `tocando` — e o cartão não tem botão
  // que o tocasse depois.
  const tetoDoCartao = b.CARTAO.bytesDeSomDecodificado;
  confere(caso, Number.isFinite(tetoDoCartao) && tetoDoCartao > 0, `o cartão não tem teto para o som decodificado: ${tetoDoCartao}`);
  if (Number.isFinite(tetoDoCartao) && tetoDoCartao > 0) {
    const quadros = Math.floor(tetoDoCartao / 8) + 1;
    const Audio = audioDeMentira({ decodificado: { duration: quadros / 48000, length: quadros, numberOfChannels: 2 } });
    const bs = bancada({ audio: Audio });
    const ds = dono(bs, () => Promise.resolve({ uri: "data:audio/ogg;base64,AA", papel: "som", bytes: 12 }));
    const r = new bs.RegiaoDeMod("a/b", ds.api, bs.raiz());
    r.declararCartoes({ 7: [{ forma: "midia", chave: "c", fonte: "som/c.ogg", tocando: true }] });
    await assentar();
    confere(
      caso,
      (Audio.ultimo?.ganhos.length ?? 0) === 0 && (Audio.ultimo?.fontes.length ?? 0) === 0,
      `o som de cartão acima do teto decodificado virou tocador: ${Audio.ultimo?.ganhos.length} ganho(s), ${Audio.ultimo?.fontes.length} fonte(s)`,
    );
    confere(
      caso,
      ds.ditos.some((e) => e.nome === "midia" && e.chave === "c" && e.estado === "recusada" && /teto/.test(e.porque ?? "")),
      `o MOD não soube que o som do cartão passou do teto decodificado: ${JSON.stringify(ds.ditos)}`,
    );
    confere(
      caso,
      ds.anotadas.some((t) => t.includes("«c» recusada:") && t.includes("decodificado")),
      `o som de cartão acima do teto decodificado não chegou ao registro como recusa: ${JSON.stringify(ds.anotadas)}`,
    );
    confere(caso, r.bytesDeSomDeCartao === 0, `o som recusado entrou na conta dos cartões: ${r.bytesDeSomDeCartao}`);
    r.soltar();
  }
}

// ---------------------------------------------------------------------------
// 7. O quadro pendente do arraste não sobrevive à saída.
// ---------------------------------------------------------------------------

async function oQuadroPendenteNaoSobreviveASaida() {
  const caso = "o quadro pendente não sobrevive à saída";
  const b = bancada();
  const d = dono(b);
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar([{ forma: "tela", chave: "t", largura: 50, altura: 50 }]);
  const tela = regiao.raiz.children[0];
  tela.pincel = { clearRect() {}, beginPath() {}, moveTo() {}, lineTo() {}, stroke() {} };

  tela.disparar("pointerdown", { clientX: 1, clientY: 1, pointerId: 1 });
  tela.disparar("pointermove", { clientX: 2, clientY: 2, pointerId: 1 });
  confere(caso, b.quadros.length === 1, "o movimento não agendou quadro nenhum");

  regiao.soltar();
  const antes = d.ditos.length;
  // O quadro que o navegador chamaria mesmo assim.
  b.quadros[0]?.();
  confere(
    caso,
    d.ditos.length === antes,
    "o quadro pendente falou com o MOD depois de a sessão ter acabado",
  );
}

// ---------------------------------------------------------------------------
// 8. Todo recurso montado chega à instância, e soltar duas vezes não dói.
// ---------------------------------------------------------------------------

async function tudoChegaAInstanciaESoltarDuasVezesNaoDoi() {
  const caso = "tudo chega à instância";
  const b = bancada();
  const d = dono(b, () =>
    Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 8 }),
  );
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar([
    { forma: "campo", chave: "c", rotulo: "X" },
    { forma: "botao", chave: "b", dentro: "GRAVAR" },
    { forma: "tela", chave: "t", largura: 10, altura: 10 },
    { forma: "midia", chave: "m", fonte: "som/a.wav" },
  ]);
  await volta();
  await volta();

  confere(
    caso,
    d.registrados.length === 4,
    `quatro recursos montados e ${d.registrados.length} registrados na instância: ` +
      JSON.stringify(d.registrados.map((r) => r.porque)),
  );

  // O caminho da instância: ela solta o que registrou, sem a região.
  for (const recurso of d.registrados) recurso.descartar();
  confere(caso, regiao.contagem.midias === 0, "a conta de mídias não voltou a zero");
  confere(caso, regiao.contagem.campos === 0, "a conta de campos não voltou a zero");

  // E a região soltando depois não solta duas vezes o mesmo.
  regiao.soltar();
  confere(
    caso,
    regiao.contagem.campos === 0 && regiao.contagem.telas === 0,
    `soltar duas vezes descontou de novo: ${JSON.stringify(regiao.contagem)}`,
  );
}

function aPreviaNaoTrocaOsAncestraisDoCampo() {
  const b = bancada();
  const r = new b.RegiaoDeMod("teste/perfil", dono(b).api, b.raiz());
  const arvore = (preenchida) => [
    { forma: "pilha", dentro: preenchida
      ? [{ forma: "caixa", dentro: "Perfil" }, { forma: "distintivo", dentro: "Status" }]
      : [] },
    { forma: "formulario", chave: "perfil", dentro: [
      { forma: "caixa", dentro: [
        { forma: "campo", chave: "status", rotulo: "STATUS", valor: "Te" },
      ] },
    ] },
  ];
  r.aplicar(arvore(false));
  const campo = acharTag(r.raiz, "input");
  b.doc.activeElement = campo;
  r.aplicar(arvore(true));
  confere("prévia e digitação", acharTag(r.raiz, "input") === campo
    && b.doc.activeElement === campo, "a prévia recriou o campo ou um ancestral");
  r.soltar();
}

function oErroDoCampoAcompanhaAEdicaoSemRoubarFoco() {
  const b = bancada(); const r = new b.RegiaoDeMod("teste/erro", dono(b).api, b.raiz());
  const no = { forma: "campo", chave: "status", valor: "rascunho" };
  r.aplicar([no]); const campo = acharTag(r.raiz, "input");
  b.doc.activeElement = campo;
  r.aplicar([{ ...no, valor: "não sobrescrever", erro: "Status: use até 60 caracteres." }]);
  confere("erro no campo", campo.getAttribute("aria-invalid") === "true"
    && campo.value === "rascunho" && b.doc.activeElement === campo,
    "o erro não apareceu com foco ou sobrescreveu a edição");
  r.aplicar([no]);
  confere("erro no campo", !campo.getAttribute("aria-invalid"), "corrigir conservou a marca de inválido");
  r.soltar();
}

function oRodapeEReconciliadoSemMoverOBotao() {
  const b = bancada();
  const d = dono(b);
  const r = new b.RegiaoDeMod("teste/rodape", d.api, b.raiz());
  r.rodape = b.raiz();
  const acoes = { forma: "acoes", fixas: true, dentro: [
    { forma: "botao", chave: "gravar", dentro: "GRAVAR" },
  ] };
  r.aplicar([acoes]);
  const botao = acharTag(r.rodape, "button");
  confere("rodapé montado", Boolean(botao) && r.raiz.children.length === 0,
    "as ações não foram destinadas ao rodapé");
  b.doc.activeElement = botao;
  for (let n = 0; n < 100; n += 1) {
    r.aplicar([{ forma: "texto", dentro: "atualização " + n }, acoes]);
  }
  confere("rodapé estável", acharTag(r.rodape, "button") === botao
    && b.doc.activeElement === botao, "o botão foi trocado ou perdeu o foco");
  confere("recursos do rodapé", r.recursos.size === 1,
    `${r.recursos.size} recursos para um botão`);
  botao?.disparar("click");
  confere("clique no rodapé", d.ditos.length === 1, "o clique não chegou exatamente uma vez");
  r.aplicar([]);
  confere("retirar rodapé", r.recursos.size === 0 && r.rodape.hidden
    && r.rodape.children.length === 0 && botao.ouvintesDePe() === 0,
    "retirar as ações reteve nós ou ouvintes");
  r.aplicar([acoes]);
  r.soltar();
  confere("sair com rodapé", r.recursos.size === 0 && r.rodape.children.length === 0,
    "encerrar não soltou o rodapé");
}

async function fundoTrocaSoltaECancela() {
  const b = bancada(); const pendentes = [];
  const d = dono(b, pedido => new Promise(resolve => pendentes.push({ pedido, resolve })));
  const r = new b.RegiaoDeMod("teste/fundo", d.api, b.raiz());
  const arvore = path => [{ forma: "caixa", chave: "pessoa", fundoDeMidia: { doServidor: { pedido: { path } } }, dentro: "Lia" }];
  r.aplicar(arvore("antiga"));
  const antigo = r.raiz.children[0];
  r.aplicar(arvore("nova"));
  const novo = r.raiz.children[0];
  pendentes[0].resolve({ papel: "imagem", uri: "data:image/png;base64,ANTIGA", bytes: 5 });
  pendentes[1].resolve({ papel: "imagem", uri: "data:image/png;base64,NOVA", bytes: 7 });
  await volta();
  confere("fundo atualizado", antigo !== novo && novo.style.backgroundImage.includes("NOVA") && !antigo.style.backgroundImage, "a foto antiga venceu a atualização");
  confere("bytes do fundo", r.bytesDeMidia === 7, "contabilidade da mídia incorreta");
  r.aplicar(arvore("pendente"));
  r.soltar();
  pendentes[2].resolve({ papel: "imagem", uri: "data:image/png;base64,TARDE", bytes: 9 });
  await volta();
  confere("fundo descartado", r.bytesDeMidia === 0 && r.recursos.size === 0, "sair reteve bytes ou recurso");
}

// ---------------------------------------------------------------------------
// 9. Cada mídia recusada é dita a quem hospeda, com o motivo.
// ---------------------------------------------------------------------------

/**
 * **O evento é do MOD; o registro é de quem investiga.** Em 23/09 o avatar do
 * PERFIS não aparecia, a janela sabia por quê, e o `seele.log` não tinha uma
 * palavra — levou uma hora de medição e um reinício com `RUST_LOG=debug`.
 *
 * Ao menos um caso por caminho de recusa, que são quinze: os dez em que a
 * janela recusa ou vê a carga falhar uma mídia (a mídia, o retrato, o fundo e
 * o fundo de tela), o evento `error` do próprio elemento, a mídia declarada
 * sem origem, e os três do som desde que ele toca por WebAudio — os bytes que
 * o Rust recusou, o áudio da janela que não liga e a fonte que não abre. O som
 * que não decodifica tem prova própria (`oSomQueNaoDecodificaEDito`). Cada um
 * mede o texto que chega a `anotarRecusa`, que o `base.js` leva ao
 * `registrar_da_janela` como WARN e com o id do MOD em campo próprio (guarda
 * irmão em `tests/frontend.rs`). A recusa do Rust entra aqui como ela chega de
 * verdade, `{ Recusado: { motivo } }`, nos cinco lugares que pedem mídia à
 * ponte (a mídia, o retrato, o fundo, o fundo de tela e os bytes do som): cada
 * um tem o seu `catch`, e um que voltasse a escrever «[object Object]» passaria
 * com os outros quatro verdes.
 *
 * E o avesso, no som: dizer não é repetir. O mesmo motivo sai uma vez por
 * tocador, e não uma por alternância do `tocando`; a recusa que chega depois de
 * a região sair, ou de o MOD tirar o nó, não sai — nem ao registro, nem ao
 * MOD, nem à anotação que o diagnóstico lê. E a chave do MOD entra cortada,
 * para que o motivo caiba nos 512 caracteres que o registro guarda.
 */
async function cadaMidiaRecusadaEDitaAoAnfitriao() {
  const recusaDoRust = (motivo) => () => () => Promise.reject({ Recusado: { motivo } });
  const casos = [
    {
      nome: "mídia acima do teto da região",
      midia: (b) => () => Promise.resolve({ uri: "x:", papel: "imagem", bytes: b.LIMITES.bytesDeMidia + 1 }),
      declarar: [{ forma: "midia", chave: "grande", fonte: "img/g.png" }],
      espera: ["«grande»", "teto"],
    },
    {
      nome: "mídia que o Rust recusou",
      midia: recusaDoRust("arquivo-nao-declarado"),
      declarar: [{ forma: "midia", chave: "sumida", fonte: "img/s.png" }],
      espera: ["«sumida»", "arquivo-nao-declarado"],
    },
    {
      nome: "retrato que não é imagem",
      midia: () => () => Promise.resolve({ uri: "x:", papel: "som", bytes: 2 }),
      declarar: [{ forma: "retrato", chave: "rosto", fonte: "som/a.wav", inicial: "A" }],
      espera: ["«rosto»", "som"],
    },
    {
      nome: "retrato acima do teto",
      midia: (b) => () => Promise.resolve({ uri: "x:", papel: "imagem", bytes: b.LIMITES.bytesDeMidia + 1 }),
      declarar: [{ forma: "retrato", chave: "rosto", fonte: "img/r.png", inicial: "A" }],
      espera: ["«rosto»", "teto"],
    },
    {
      nome: "retrato que o Rust recusou",
      midia: recusaDoRust("formato-desconhecido"),
      declarar: [{ forma: "retrato", chave: "rosto", fonte: "img/r.png", inicial: "A" }],
      espera: ["«rosto»", "formato-desconhecido"],
    },
    {
      nome: "fundo que não é imagem",
      midia: () => () => Promise.resolve({ uri: "x:", papel: "som", bytes: 2 }),
      declarar: [{ forma: "caixa", chave: "cx", fundoDeMidia: { fonte: "som/a.wav" }, dentro: "Lia" }],
      espera: ["«cx»", "som"],
    },
    {
      nome: "fundo acima do teto",
      midia: (b) => () => Promise.resolve({ uri: "x:", papel: "imagem", bytes: b.LIMITES.bytesDeMidia + 1 }),
      declarar: [{ forma: "caixa", chave: "cx", fundoDeMidia: { fonte: "img/f.png" }, dentro: "Lia" }],
      espera: ["«cx»", "teto"],
    },
    // O mesmo `catch` do caso de baixo, com a recusa como o Rust a manda: sem
    // este, o fundo podia voltar a escrever «[object Object]» com todo o resto
    // verde, porque o caso de baixo rejeita com um `Error`.
    {
      nome: "fundo que o Rust recusou",
      midia: recusaDoRust("arquivo-nao-declarado"),
      declarar: [{ forma: "caixa", chave: "cx", fundoDeMidia: { fonte: "img/f.png" }, dentro: "Lia" }],
      espera: ["«cx»", "arquivo-nao-declarado"],
    },
    {
      nome: "fundo que falhou na janela",
      midia: () => () => Promise.reject(new Error("a resposta do servidor não traz «image»")),
      declarar: [{ forma: "caixa", chave: "cx", fundoDeMidia: { fonte: "img/f.png" }, dentro: "Lia" }],
      espera: ["«cx»", "não traz"],
    },
  ];
  for (const caso of casos) {
    const b = bancada();
    const d = dono(b, caso.midia(b));
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar(caso.declarar);
    await volta();
    await volta();
    confere(
      `recusa dita · ${caso.nome}`,
      d.anotadas.some((texto) => caso.espera.every((pedaco) => texto.includes(pedaco))),
      `nada chegou ao registro com ${caso.espera.join(" e ")}: ${JSON.stringify(d.anotadas)}`,
    );
    confere(
      `recusa dita · ${caso.nome}`,
      !d.anotadas.some((texto) => texto.includes("[object Object]")),
      `a recusa do Rust virou «[object Object]»: ${JSON.stringify(d.anotadas)}`,
    );
    regiao.soltar();
  }

  // A chave é do MOD, e a região não lhe põe teto. O registro corta a frase
  // inteira em 512 caracteres (`TETO_DA_FRASE_NO_REGISTRO`, `main.rs`), já com
  // o `autor/nome: ` que o `base.js` põe na frente — aqui o id é o da região,
  // `a/b`. Uma chave de quinhentos empurrava o motivo para fora da linha; e um
  // corte por índice que parta um par substituto deixa a frase malformada, que
  // a ponte recusa inteira.
  {
    const TETO_DA_FRASE_NO_REGISTRO = 512;
    const b = bancada();
    const d = dono(b, recusaDoRust("arquivo-nao-declarado")(b));
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([
      { forma: "midia", chave: "x".repeat(500), fonte: "img/x.png" },
      { forma: "midia", chave: `${"y".repeat(119)}😀${"y".repeat(10)}`, fonte: "img/y.png" },
    ]);
    await volta();
    await volta();
    const longa = d.anotadas.find((texto) => texto.includes("«xxxx"));
    const comoORegistroGuarda = longa === undefined
      ? ""
      : Array.from(`a/b: ${longa}`).slice(0, TETO_DA_FRASE_NO_REGISTRO).join("");
    confere(
      "recusa dita · chave longa",
      comoORegistroGuarda.includes("arquivo-nao-declarado"),
      "a chave do MOD, sem corte, empurrou o motivo para fora dos 512 caracteres que o registro "
        + `guarda da frase, e a linha diz de quem é sem dizer por quê: ${JSON.stringify(comoORegistroGuarda)}`,
    );
    const partida = d.anotadas.find((texto) => texto.includes("«yyyy"));
    confere(
      "recusa dita · chave longa",
      partida !== undefined && partida.isWellFormed(),
      "o corte da chave partiu um par substituto ao meio, e a ponte recusa a frase malformada "
        + `inteira — a recusa não chega ao seele.log: ${JSON.stringify(d.anotadas)}`,
    );
    regiao.soltar();
  }

  // O fundo de uma tela: ele só é buscado quando a tela tem onde pintar, e a
  // primeira montagem ainda não tem pincel (`pintarTela` sai antes de
  // `buscarFundoDaTela` quando `getContext` não devolve nada).
  for (const [nome, midia, pedaco] of [
    [
      "fundo de tela que não é imagem",
      () => Promise.resolve({ uri: "x:", papel: "som", bytes: 2 }),
      "som",
    ],
    [
      "fundo de tela que o Rust recusou",
      () => Promise.reject({ Recusado: { motivo: "arquivo-nao-declarado" } }),
      "arquivo-nao-declarado",
    ],
  ]) {
    const b = bancada();
    const d = dono(b, midia);
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    const declarar = () => regiao.aplicar([
      { forma: "tela", chave: "mapa", largura: 10, altura: 10, fundo: { fonte: "img/mapa.png" } },
    ]);
    declarar();
    regiao.raiz.children[0].darPincel();
    declarar();
    await volta();
    await volta();
    confere(
      `recusa dita · ${nome}`,
      d.anotadas.some((texto) => texto.includes("«mapa»") && texto.includes(pedaco)),
      `o fundo recusado da tela não chegou ao registro: ${JSON.stringify(d.anotadas)}`,
    );
    confere(
      `recusa dita · ${nome}`,
      !d.anotadas.some((texto) => texto.includes("[object Object]")),
      `a recusa do Rust virou «[object Object]»: ${JSON.stringify(d.anotadas)}`,
    );
    regiao.soltar();
  }

  // Os bytes de um som do pacote, pedidos a `som_do_mod` por `dono.bytesDoSom`:
  // o quinto lugar que pede mídia à ponte, com o `catch` dele. A recusa do Rust
  // entra como ela chega, `{ Recusado: { motivo } }`.
  {
    const b = bancada({ audio: audioDeMentira() });
    const d = dono(b, () => Promise.resolve({ uri: "x:", papel: "som", bytes: 2 }));
    d.api.bytesDoSom = () => Promise.reject({ Recusado: { motivo: "formato-desconhecido" } });
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([{ forma: "midia", chave: "m", fonte: "som/a.wav" }]);
    await assentar();
    confere(
      "recusa dita · som que o Rust recusou",
      d.anotadas.some((texto) => texto.includes("«m»") && texto.includes("formato-desconhecido")),
      `os bytes do som que o Rust recusou não chegaram ao registro com o motivo: ${JSON.stringify(d.anotadas)}`,
    );
    confere(
      "recusa dita · som que o Rust recusou",
      !d.anotadas.some((texto) => texto.includes("[object Object]")),
      `a recusa do Rust virou «[object Object]»: ${JSON.stringify(d.anotadas)}`,
    );
    regiao.soltar();
  }

  // O som que o navegador não deixou tocar. Sem `<audio>` desde a fase M1 da
  // casca, não há `play()` que rejeite: o equivalente é o áudio da janela que
  // não liga sem um gesto de quem usa, e o tocador diz «recusada» depois do
  // prazo (`ESPERA_SEM_GESTO_MS`, em `mods-regiao.js`).
  //
  // Na mesma espera, dois sons que saem antes do prazo, com o pedido de tocar
  // ainda pendente: um numa região que sai inteira, e outro cujo MOD tirou o
  // nó. A recusa que chega depois não é mais de ninguém — nem do registro, nem
  // do MOD, nem da anotação que o diagnóstico lê.
  {
    const b = bancada({ audio: audioDeMentira({ semGesto: true }) });
    const d = dono(b, () => Promise.resolve({ uri: "x:", papel: "som", bytes: 2 }));
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    const declarar = (tocando) =>
      regiao.aplicar([{ forma: "midia", chave: "toque", fonte: "som/a.wav", tocando }]);
    declarar(true);

    const orfas = [];
    for (const [como, sair] of [
      ["a região saiu", (r) => r.soltar()],
      ["o MOD tirou o nó", (r) => r.aplicar([])],
    ]) {
      const Audio = audioDeMentira({ semGesto: true });
      const bo = bancada({ audio: Audio });
      const doOrfa = dono(bo, () => Promise.resolve({ uri: "x:", papel: "som", bytes: 2 }));
      const ro = new bo.RegiaoDeMod("a/b", doOrfa.api, bo.raiz());
      ro.aplicar([{ forma: "midia", chave: "orfa", fonte: "som/a.wav", tocando: true }]);
      orfas.push({ como, sair, Audio, d: doOrfa, regiao: ro });
    }
    await assentar();
    for (const orfa of orfas) {
      confere(
        `recusa dita · som recusado depois que ${orfa.como}`,
        (orfa.Audio.ultimo?.acordar ?? 0) > 0,
        "o som nem pediu para tocar, e as conferências abaixo passariam por não medir nada",
      );
      orfa.sair(orfa.regiao);
    }
    // O prazo é de um segundo e meio; espera-se um pouco mais.
    await new Promise((r) => setTimeout(r, 1700));
    confere(
      "recusa dita · som que não tocou",
      d.anotadas.some((texto) => texto.includes("«toque»") && texto.includes("gesto")),
      `o som que o navegador não deixou tocar não chegou ao registro: ${JSON.stringify(d.anotadas)}`,
    );
    for (const orfa of orfas) {
      const caso = `recusa dita · som recusado depois que ${orfa.como}`;
      confere(
        caso,
        !orfa.d.anotadas.some((texto) => texto.includes("«orfa»")),
        `a recusa que chegou depois ainda escreveu no seele.log: ${JSON.stringify(orfa.d.anotadas)}`,
      );
      confere(
        caso,
        !orfa.d.ditos.some((e) => e.nome === "midia" && e.estado === "recusada"),
        `o MOD ouviu «recusada» de um som que já tinha saído da tela: ${JSON.stringify(orfa.d.ditos)}`,
      );
      confere(
        caso,
        orfa.regiao.midiasAnotadas().length === 0,
        "a recusa que chegou depois recriou a anotação de um nó que já saiu, e o diagnóstico "
          + `conta uma mídia que não existe: ${JSON.stringify(orfa.regiao.midiasAnotadas())}`,
      );
    }

    // **Uma linha por motivo, e não uma por alternância.** O MOD que liga e
    // desliga o som sem um gesto de quem usa: cada `tocando` que volta a ser
    // verdadeiro pede de novo, e cada pedido recusado é dito ao MOD e anotado —
    // mas o registro já tem o motivo, e o `seele.log`, que só gira na abertura,
    // não ganha uma linha por alternância. Os redesenhos que não mudam o
    // `tocando` nem pedem.
    declarar(false);
    declarar(true);
    declarar(true);
    await new Promise((r) => setTimeout(r, 1700));
    const doToque = d.anotadas.filter((texto) => texto.includes("«toque»"));
    confere(
      "recusa dita · som que não tocou",
      doToque.length === 1,
      "o mesmo motivo de recusa virou uma linha por alternância do `tocando`, e um MOD que "
        + `liga e desliga o som soterra o seele.log: ${JSON.stringify(doToque)}`,
    );
    const recusadas = d.ditos.filter((e) => e.nome === "midia" && e.chave === "toque" && e.estado === "recusada");
    confere(
      "recusa dita · som que não tocou",
      recusadas.length === 2,
      "o MOD tinha de ouvir «recusada» a cada pedido recusado, e a conferência acima passaria "
        + `por não ter pedido de novo: ${JSON.stringify(d.ditos)}`,
    );
    const anotada = regiao.midiasAnotadas();
    confere(
      "recusa dita · som que não tocou",
      anotada.length === 1 && anotada[0].situacao === "recusada" && /gesto/.test(anotada[0].motivo),
      "o som que não tocou não ficou anotado como recusado, e o diagnóstico conta como pronta "
        + `uma mídia muda: ${JSON.stringify(anotada)}`,
    );
    regiao.soltar();
  }

  // O pedido de tocar que **rejeita** em vez de avisar: a fonte que o WebAudio
  // não abre. Um WebAudio conforme não chega aqui, e é por isso mesmo que o
  // caminho tem prova: um `catch` que nada exercita é o que volta a ficar mudo.
  // A recusa vai ao MOD e ao registro pelo mesmo caminho da do prazo.
  {
    const b = bancada({ audio: audioDeMentira({ fonteLanca: true }) });
    const d = dono(b, () => Promise.resolve({ uri: "x:", papel: "som", bytes: 2 }));
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([{ forma: "midia", chave: "muda", fonte: "som/a.wav", tocando: true }]);
    await assentar();
    confere(
      "recusa dita · som cuja fonte não abriu",
      d.ditos.some((e) => e.nome === "midia" && e.chave === "muda" && e.estado === "recusada"),
      `o MOD não soube que o som não começou: ${JSON.stringify(d.ditos)}`,
    );
    confere(
      "recusa dita · som cuja fonte não abriu",
      d.anotadas.some((texto) => texto.includes("«muda»") && texto.includes("a fonte não abriu")),
      `a fonte que não abriu não chegou ao registro: ${JSON.stringify(d.anotadas)}`,
    );
    // E pelo clique, o mesmo caminho: o MOD ouve de novo, e o registro, que já
    // tem o motivo, não ganha outra linha.
    acharTag(regiao.raiz, "button")?.disparar("click");
    await assentar();
    const recusadas = d.ditos.filter((e) => e.nome === "midia" && e.chave === "muda" && e.estado === "recusada");
    confere(
      "recusa dita · som cuja fonte não abriu",
      recusadas.length === 2,
      `o clique que não tocou não foi dito ao MOD como «recusada»: ${JSON.stringify(d.ditos)}`,
    );
    const daMuda = d.anotadas.filter((texto) => texto.includes("«muda»"));
    confere(
      "recusa dita · som cuja fonte não abriu",
      daMuda.length === 1,
      `a mesma fonte que não abriu virou uma linha por pedido: ${JSON.stringify(daMuda)}`,
    );
    regiao.soltar();
  }

  // O clique de quem usa, e o áudio da janela que mesmo assim não liga. A linha
  // não diz «sem um gesto»: houve um, e dizer o contrário manda quem investiga
  // procurar no lugar errado. E o áudio que liga depois devolve a mídia a
  // «pronta»: a recusa de antes não fica anotada para sempre.
  {
    const Audio = audioDeMentira({ recusaNaHora: true });
    const b = bancada({ audio: Audio });
    const d = dono(b, () => Promise.resolve({ uri: "x:", papel: "som", bytes: 2 }));
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([{ forma: "midia", chave: "clique", fonte: "som/a.wav", descricao: "Sino" }]);
    await assentar();
    const botao = acharTag(regiao.raiz, "button");
    confere(
      "recusa dita · som que o clique não ligou",
      Boolean(botao),
      "o botão do produto não foi montado, e as conferências abaixo passariam por não clicar",
    );
    botao?.disparar("click");
    await assentar();
    const doClique = d.anotadas.filter((texto) => texto.includes("«clique»"));
    confere(
      "recusa dita · som que o clique não ligou",
      doClique.length === 1 && !doClique[0].includes("sem um gesto"),
      `o clique de quem usa foi dito ao registro como «sem um gesto», ou não foi dito: ${JSON.stringify(doClique)}`,
    );
    // **E o motivo do navegador vai junto.** `resume()` recusou com um motivo
    // próprio, e ele é o que quem investiga veio buscar: engolido, a linha
    // diria que o áudio não ligou sem dizer o que o navegador respondeu.
    confere(
      "recusa dita · som que o clique não ligou",
      doClique.length === 1 && doClique[0].includes("NotAllowedError: o áudio da janela não liga"),
      `o motivo com que o áudio da janela recusou ligar não chegou ao registro: ${JSON.stringify(doClique)}`,
    );
    if (Audio.ultimo) Audio.ultimo.state = "running";
    botao?.disparar("click");
    await assentar();
    const anotada = regiao.midiasAnotadas();
    confere(
      "recusa dita · som que o clique não ligou",
      Audio.ultimo?.fontes[0]?.tocando === true && anotada.length === 1 && anotada[0].situacao === "pronta",
      "o som que passou a tocar continuou anotado como recusado, e o diagnóstico conta como "
        + `muda uma mídia que toca: ${JSON.stringify(anotada)}`,
    );
    regiao.soltar();
  }

  // A recusa que o próprio elemento faz: os bytes chegaram, o Rust os aceitou,
  // a conta coube, e o `<img>`/`<audio>` não os abriu. O Rust não vê este caso,
  // e o MOD só recebe «falhou», sem motivo.
  {
    const b = bancada();
    const d = dono(b, () => Promise.resolve({ uri: "x:", papel: "imagem", bytes: 2 }));
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([{ forma: "midia", chave: "quebrada", fonte: "img/q.png" }]);
    await volta();
    await volta();
    const tocador = regiao.raiz.querySelector(".regiao-de-mod-tocador");
    confere(
      "recusa dita · elemento que não abriu",
      tocador !== null && d.anotadas.length === 0,
      `a imagem aceita não montou, ou já foi dita como recusa antes do evento: ${JSON.stringify(d.anotadas)}`,
    );
    tocador?.disparar("error");
    confere(
      "recusa dita · elemento que não abriu",
      d.anotadas.some((texto) => texto.includes("«quebrada»") && texto.includes("error")),
      `o evento error do elemento não chegou ao registro: ${JSON.stringify(d.anotadas)}`,
    );
    regiao.soltar();
  }

  // A mídia declarada sem nenhuma das duas origens: a janela marca «sem-fonte»
  // no elemento, e o MOD não recebe evento nenhum.
  {
    const b = bancada();
    const d = dono(b);
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([{ forma: "midia", chave: "vazia" }]);
    confere(
      "recusa dita · mídia sem origem",
      d.anotadas.some((texto) => texto.includes("«vazia»") && texto.includes("«fonte»")),
      `a mídia sem origem não chegou ao registro: ${JSON.stringify(d.anotadas)}`,
    );
    regiao.soltar();
  }
}

// ---------------------------------------------------------------------------
// 10. A região diz em que pé cada mídia está — para o diagnóstico ler.
// ---------------------------------------------------------------------------

async function aMidiaAnotadaDizOsTresEstados() {
  const caso = "a mídia anotada";
  const b = bancada();
  let chegar;
  const pendente = new Promise((r) => {
    chegar = r;
  });
  const d = dono(b, (caminho) => {
    if (caminho === "img/falta.png") {
      // A forma em que o Rust recusa: o enum serializado, e não um `Error`.
      return Promise.reject({ Recusado: { motivo: "arquivo-nao-declarado" } });
    }
    if (caminho === "img/grande.png") {
      return Promise.resolve({
        uri: "data:image/png;base64,AA",
        papel: "imagem",
        bytes: b.LIMITES.bytesDeMidia + 1,
      });
    }
    return pendente;
  });
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar([
    { forma: "midia", chave: "a", fonte: "img/a.png" },
    { forma: "midia", chave: "falta", fonte: "img/falta.png" },
    { forma: "midia", chave: "nada" },
    { forma: "midia", chave: "grande", fonte: "img/grande.png" },
  ]);
  await assentar();
  const situacoes = () => regiao.midiasAnotadas().map((m) => m.situacao).sort().join(",");
  confere(
    caso,
    situacoes() === "carregando,recusada,recusada,recusada",
    `as quatro mídias não disseram em que pé estão: ${situacoes()}`,
  );
  const motivos = regiao.midiasAnotadas().map((m) => m.motivo).join(" | ");
  confere(caso, motivos.includes("arquivo-nao-declarado"), `a recusa do Rust virou outra coisa: ${motivos}`);
  confere(caso, !motivos.includes("[object Object]"), `a recusa do Rust virou «[object Object]»: ${motivos}`);
  confere(caso, motivos.includes("fonte"), `a mídia sem origem foi recusada sem dizer por quê: ${motivos}`);
  confere(caso, motivos.includes("teto"), `a mídia acima do teto foi recusada sem dizer por quê: ${motivos}`);

  chegar({ uri: "data:image/png;base64,AA", papel: "imagem", bytes: 10 });
  await assentar();
  confere(
    caso,
    situacoes() === "pronta,recusada,recusada,recusada",
    `a mídia que chegou não virou «pronta»: ${situacoes()}`,
  );
  confere(caso, d.mudancas() >= 5, `a região mudou o estado das mídias e avisou ${d.mudancas()} vez(es)`);

  // A imagem que chegou, coube e o navegador não abriu: «pronta» vira
  // «recusada». Sem isto, o diagnóstico contaria de pé um quadrado quebrado.
  acharTag(regiao.raiz, "img")?.disparar("error");
  const quebrada = regiao.midiasAnotadas().find((m) => /não decodificou/.test(m.motivo));
  confere(
    caso,
    situacoes() === "recusada,recusada,recusada,recusada" && quebrada?.situacao === "recusada",
    "a imagem que o navegador não abriu continuou anotada como «pronta», e o diagnóstico conta "
      + `de pé uma mídia quebrada: ${JSON.stringify(regiao.midiasAnotadas())}`,
  );

  // O que sai da declaração sai do diagnóstico: contar mídia que não existe
  // mais é o diagnóstico mentindo.
  regiao.aplicar([{ forma: "midia", chave: "falta", fonte: "img/falta.png" }]);
  confere(
    caso,
    regiao.midiasAnotadas().length === 1,
    `sobraram ${regiao.midiasAnotadas().length} anotações para uma mídia de pé`,
  );
  regiao.soltar();
  confere(caso, regiao.midiasAnotadas().length === 0, "a região solta continuou anotando mídia");

  // O motivo anotado é cortado em 200 **pontos de código**, como a chave em
  // `dizerRecusaDeMidia`: um corte por índice que caia no meio de um par
  // substituto deixa metade de um caractere no motivo, e a aba do diagnóstico
  // mostra lixo no lugar do fim da frase.
  const b2 = bancada();
  const longo = `${"x".repeat(199)}😀${"x".repeat(20)}`;
  const d2 = dono(b2, () => Promise.reject({ Recusado: { motivo: longo } }));
  const regiao2 = new b2.RegiaoDeMod("a/b", d2.api, b2.raiz());
  regiao2.aplicar([{ forma: "midia", chave: "longa", fonte: "img/l.png" }]);
  await assentar();
  const cortado = regiao2.midiasAnotadas()[0]?.motivo ?? "";
  confere(
    caso,
    cortado.isWellFormed() && cortado.endsWith("😀") && Array.from(cortado).length === 200,
    `o corte do motivo anotado partiu um par substituto ao meio, ou não cortou em 200: ${JSON.stringify(cortado.slice(-4))}`,
  );
  regiao2.soltar();
}

async function oFundoEORetratoTambemDizemOEstado() {
  const caso = "fundo e retrato anotados";
  const b = bancada();
  const d = dono(b, (caminho) => Promise.resolve(caminho === "som/a.wav"
    ? { uri: "data:audio/wav;base64,AA", papel: "som", bytes: 8 }
    : { uri: "data:image/png;base64,AA", papel: "imagem", bytes: 8 }));
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar([
    {
      forma: "caixa",
      chave: "c",
      fundoDeMidia: { fonte: "img/fundo.png" },
      dentro: [{ forma: "texto", dentro: "x" }],
    },
    { forma: "retrato", chave: "r", fonte: "img/rosto.png" },
    { forma: "retrato", chave: "s", fonte: "som/a.wav" },
  ]);
  await assentar();
  const situacoes = regiao.midiasAnotadas().map((m) => m.situacao).sort().join(",");
  confere(
    caso,
    situacoes === "pronta,pronta,recusada",
    `o fundo e os retratos não disseram em que pé estão: ${situacoes}`,
  );
  const recusa = regiao.midiasAnotadas().find((m) => m.situacao === "recusada");
  confere(caso, /imagem/.test(recusa?.motivo ?? ""), `o retrato de som foi recusado sem dizer por quê: ${recusa?.motivo}`);
  // O fundo e o retrato que saem da declaração saem do diagnóstico antes de a
  // região sair: o `soltar()` abaixo limpa tudo, e por isso não vê o descarte
  // de cada um esquecendo a sua anotação.
  regiao.aplicar([{ forma: "retrato", chave: "r", fonte: "img/rosto.png" }]);
  confere(
    caso,
    regiao.midiasAnotadas().length === 1,
    `o fundo ou o retrato que saiu da declaração continuou anotado: ${JSON.stringify(regiao.midiasAnotadas())}`,
  );
  regiao.soltar();
  confere(caso, regiao.midiasAnotadas().length === 0, "a região solta continuou anotando o fundo ou o retrato");

  // **Cada recusa do fundo e do retrato é anotada, com o motivo** (I-5 da
  // revisão ampla do Plano 1D). Sem a anotação, a do começo fica valendo: a
  // aba DIAGNÓSTICO diria «1 carregando», sem motivo, de algo que o Rust ou o
  // teto já recusou — e só o `seele.log` saberia. Uma região por caso: a
  // anotação não diz de que nó é, e com uma só ela é a do caso.
  const imagem = (bytes) => ({ uri: "data:image/png;base64,AA", papel: "imagem", bytes });
  const recusaDoRust = () => Promise.reject({ Recusado: { motivo: "arquivo-nao-declarado" } });
  const fundo = { forma: "caixa", chave: "c", fundoDeMidia: { fonte: "img/fundo.png" }, dentro: [{ forma: "texto", dentro: "x" }] };
  const retrato = { forma: "retrato", chave: "r", fonte: "img/rosto.png" };
  const recusas = {
    "o fundo que não é imagem": {
      no: fundo,
      midia: () => Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 8 }),
      motivo: /imagem/,
    },
    "o fundo acima do teto": { no: fundo, midia: (bb) => Promise.resolve(imagem(bb.PERFIS.regiao.bytesDeMidia + 1)), motivo: /teto/ },
    "o fundo que o Rust recusou": { no: fundo, midia: recusaDoRust, motivo: /arquivo-nao-declarado/ },
    "o retrato acima do teto": { no: retrato, midia: (bb) => Promise.resolve(imagem(bb.PERFIS.regiao.bytesDeMidia + 1)), motivo: /teto/ },
    "o retrato que o Rust recusou": { no: retrato, midia: recusaDoRust, motivo: /arquivo-nao-declarado/ },
  };
  for (const [nome, recusa] of Object.entries(recusas)) {
    const br = bancada();
    const dr = dono(br, () => recusa.midia(br));
    const umaRegiao = new br.RegiaoDeMod("a/b", dr.api, br.raiz());
    umaRegiao.aplicar([recusa.no]);
    await assentar();
    const anotadas = umaRegiao.midiasAnotadas();
    confere(
      `${caso} · ${nome}`,
      anotadas.length === 1 && anotadas[0].situacao === "recusada" && recusa.motivo.test(anotadas[0].motivo),
      `${nome} não ficou anotado «recusada» com o motivo (${recusa.motivo}): ${JSON.stringify(anotadas)} — a aba `
        + "DIAGNÓSTICO diria «carregando» de algo que já foi recusado",
    );
    umaRegiao.soltar();
  }
}

// ---------------------------------------------------------------------------
// 11. O som toca por WebAudio, com botão do produto, e a declaração vale na mudança.
// ---------------------------------------------------------------------------

async function oSomTocaPorWebAudioENuncaPorUmElementoDeMidia() {
  const caso = "o som toca por WebAudio";
  const Audio = audioDeMentira();
  const b = bancada({ audio: Audio });
  const d = dono(b, () =>
    Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 12 }),
  );
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  const declaracao = [{ forma: "midia", chave: "m", fonte: "som/a.wav", descricao: "Sino" }];
  regiao.aplicar(declaracao);
  await assentar();

  const figura = regiao.raiz.children[0];
  confere(caso, acharTag(figura, "audio") === null, "um `<audio>` voltou a ser montado, e a CSP recusa o `data:` dele");
  confere(caso, d.pedidosDeSom.join() === "som/a.wav", `os bytes do som não foram pedidos pelo caminho declarado: ${d.pedidosDeSom}`);
  confere(caso, figura.dataset.estado === "pronta", `o som decodificado não ficou pronto: ${figura.dataset.estado}`);
  // **De pé e sem ter tocado, ele está pronto.** É o caso comum — um botão à
  // espera, um som que o MOD dispara depois —, e quem diz «pronta» aqui é a
  // anotação do fim de `montarSom`: os avisos do tocador só chegam quando ele
  // toca. Sem ela, a aba DIAGNÓSTICO diria «carregando» até alguém apertar.
  const anotada = regiao.midiasAnotadas()[0]?.situacao;
  confere(caso, anotada === "pronta", `o som de pé que ainda não tocou ficou anotado como «${anotada}», e não «pronta»`);
  const botao = acharTag(figura, "button");
  confere(
    caso,
    Boolean(botao) && botao.className.split(/\s+/).includes("regiao-de-mod-som"),
    "o botão do produto que toca o som não foi montado",
  );
  if (!botao) return;
  confere(caso, botao.textContent === "TOCAR", `o botão começou dizendo «${botao.textContent}»`);
  confere(caso, (Audio.ultimo?.fontes.length ?? 0) === 0, "o som tocou sem ninguém pedir");

  botao.disparar("click");
  await assentar();
  const fonte = Audio.ultimo?.fontes[0];
  confere(caso, fonte?.tocando === true, "o botão do produto não tocou o som");
  confere(caso, botao.textContent === "PAUSAR", `tocando, o botão diz «${botao.textContent}»`);
  confere(caso, d.ditos.some((e) => e.nome === "midia" && e.estado === "tocando"), "o MOD não soube que o som começou");

  // O MOD redesenha **sem** mudar `tocando`: quem apertou continua ouvindo.
  regiao.aplicar(declaracao);
  confere(caso, fonte?.tocando === true, "um redesenho do MOD que não mudou `tocando` parou o som que a pessoa pediu");

  botao.disparar("click");
  confere(caso, fonte?.parou === true && botao.textContent === "TOCAR", "pausar não parou o som, ou o botão não voltou a TOCAR");
  confere(caso, d.ditos.some((e) => e.nome === "midia" && e.estado === "pausada"), "o MOD não soube que o som parou");

  // O fim natural é dito, e o botão volta ao começo.
  botao.disparar("click");
  await assentar();
  Audio.ultimo?.fontes[1]?.onended?.();
  confere(
    caso,
    d.ditos.some((e) => e.nome === "midia" && e.estado === "terminou") && botao.textContent === "TOCAR",
    "o fim do som não foi dito ao MOD, ou o botão ficou em PAUSAR",
  );

  // A declaração vale **na mudança**: `tocando` passando a verdadeiro toca.
  regiao.aplicar([{ ...declaracao[0], tocando: true }]);
  await assentar();
  confere(caso, Audio.ultimo?.fontes[2]?.tocando === true, "o MOD passou a declarar `tocando` e o som não começou");
  regiao.soltar();
}

async function oSomDoServidorVemDosBytesDoDataENaoDeUmElemento() {
  const caso = "o som do servidor";
  const Audio = audioDeMentira();
  const b = bancada({ audio: Audio });
  // «UklGRg==» são os quatro bytes de «RIFF».
  const d = dono(b, () =>
    Promise.resolve({ uri: "data:audio/wav;base64,UklGRg==", papel: "som", bytes: 4 }),
  );
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar([{ forma: "midia", chave: "m", doServidor: { canal: 1, pedido: { op: "som" } } }]);
  await assentar();
  confere(caso, d.pedidosDeSom.length === 0, "o som do servidor foi pedido ao pacote");
  confere(caso, Audio.decodificados[0] === 4, `os bytes do data: não chegaram ao WebAudio: ${Audio.decodificados}`);
  confere(caso, regiao.raiz.children[0].dataset.estado === "pronta", "o som do servidor não ficou pronto");
  regiao.soltar();
}

async function oSomQueNaoDecodificaEDito() {
  const caso = "o som que não decodifica";
  const Audio = audioDeMentira({ naoDecodifica: true });
  const b = bancada({ audio: Audio });
  const d = dono(b, () =>
    Promise.resolve({ uri: "data:audio/ogg;base64,AA", papel: "som", bytes: 12 }),
  );
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar([{ forma: "midia", chave: "m", fonte: "som/a.ogg" }]);
  await assentar();
  const figura = regiao.raiz.children[0];
  confere(caso, figura.dataset.estado === "falhou", `a figura não disse que falhou: ${figura.dataset.estado}`);
  const dito = d.ditos.find((e) => e.nome === "midia" && e.estado === "falhou");
  confere(caso, /não decodificou/.test(dito?.porque ?? ""), `o MOD não soube por quê: ${JSON.stringify(dito)}`);
  confere(caso, regiao.midiasAnotadas()[0]?.situacao === "recusada", "o diagnóstico não anotou a recusa");
  // E quem investiga também sabe: a mesma linha que o `.catch` de
  // `montarMidia` escreve para uma mídia que o Rust recusou.
  confere(
    caso,
    d.anotadas.some((texto) => texto.includes("«m»") && texto.includes("não decodificou")),
    `o som que não decodificou não chegou ao registro: ${JSON.stringify(d.anotadas)}`,
  );
  confere(caso, acharTag(figura, "button") === null, "um botão de tocar apareceu para um som que não decodificou");
  regiao.soltar();
  confere(caso, regiao.bytesDeMidia === 0, `sobraram ${regiao.bytesDeMidia} bytes contados`);
}

async function oSomNumCartaoNaoTemBotao() {
  const caso = "o som num cartão";
  const Audio = audioDeMentira();
  const b = bancada({ audio: Audio });
  const d = dono(b, () =>
    Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 12 }),
  );
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.declararCartoes({ 7: [{ forma: "midia", chave: "m", fonte: "som/a.wav", tocando: true }] });
  // A lista de pessoas pendura o cartão na linha da pessoa no mesmo desenho
  // (`darCartoesDoMod` → `redesenharAsPessoas`), antes de os bytes chegarem.
  b.doc.body.append(regiao.cartaoDe(7));
  await assentar();
  const cartao = regiao.cartaoDe(7);
  confere(caso, acharTag(cartao, "button") === null, "um cartão ganhou um botão, e cartão não recebe foco (ver FORMAS_DO_CARTAO)");
  confere(caso, Audio.ultimo?.fontes[0]?.tocando === true, "o som que o MOD declarou no cartão não tocou");
  confere(caso, regiao.midiasAnotadas()[0]?.cartao === true, "a mídia do cartão não foi anotada como de cartão");
  regiao.soltar();
  confere(caso, Audio.ultimo?.fontes[0]?.parou === true, "o som do cartão continuou depois de a região sair");

  // **Numa contribuição, os sons moram num destino só, e cada som toca uma
  // vez.** `montarContribuicao` (`base.js`) monta um renderer por destino, com
  // o perfil do cartão — um `canal.item` sem alvo, dois canais, dois renderers
  // —, e dá a todos o mesmo `sonsDaContribuicao`. O primeiro destino que chega
  // a um som é o que pede os bytes, decodifica e toca **todos** os sons da
  // contribuição; os outros mostram a figura, sem tocador (ver `montarSom`).
  //
  // As duas metades já falharam:
  // - **um som por destino**: cada destino pedia os bytes e decodificava a
  //   sua cópia, todas de pé ao mesmo tempo — num `pessoa.cartao` sem alvo
  //   numa sala de 64 pessoas, 64 cópias de um som que pode chegar a 64 MiB
  //   decodificado —, e todos tocavam juntos;
  // - **cada som é o seu**: um conjunto pela chave do plano confundia dois
  //   sons de contêineres diferentes — a posição recomeça dentro de cada um, e
  //   os dois eram `p:midia:0.0` —, e calava o segundo sem evento, sem linha no
  //   registro e com a anotação «pronta». Uma chave do MOD repetida em dois
  //   contêineres caía no mesmo lugar.
  const conteudos = {
    "dois sons sem chave, um em cada linha": [
      { forma: "linha", dentro: [{ forma: "midia", fonte: "som/a.wav", tocando: true }] },
      { forma: "linha", dentro: [{ forma: "midia", fonte: "som/b.wav", tocando: true }] },
    ],
    "a mesma chave em duas linhas": [
      { forma: "linha", dentro: [{ forma: "midia", chave: "sino", fonte: "som/a.wav", tocando: true }] },
      { forma: "linha", dentro: [{ forma: "midia", chave: "sino", fonte: "som/b.wav", tocando: true }] },
    ],
  };
  for (const [nome, conteudo] of Object.entries(conteudos)) {
    const naContribuicao = `${caso} · numa contribuição, ${nome}`;
    const AudioC = audioDeMentira();
    const bc = bancada({ audio: AudioC });
    const dc = dono(bc, () => Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 12 }));
    const sonsDaContribuicao = { tomados: false };
    const destinos = ["1", "2"].map(() => {
      const r = new bc.RegiaoDeMod("a/b", dc.api, bc.raiz(), bc.PERFIS.cartao);
      r.sonsDaContribuicao = sonsDaContribuicao;
      r.aplicar(conteudo);
      return r;
    });
    await assentar();
    const figuras = destinos.map((r) => todasAsTags(r.raiz, "figure").map((f) => f.dataset.estado));
    confere(
      naContribuicao,
      JSON.stringify(figuras) === JSON.stringify([["pronta", "pronta"], ["pronta", "pronta"]]),
      `as figuras dos dois destinos não assentaram — um cartão ficaria dizendo «carregando» para sempre: ${JSON.stringify(figuras)}`,
    );
    const pedidos = [...dc.pedidosDeSom].sort();
    confere(
      naContribuicao,
      JSON.stringify(pedidos) === JSON.stringify(["som/a.wav", "som/b.wav"]),
      "os bytes não foram pedidos uma vez por som — a mais é uma cópia por destino, a menos é um som que "
        + `nunca toca: ${JSON.stringify(pedidos)}`,
    );
    const decodificados = AudioC.decodificados.length;
    confere(
      naContribuicao,
      decodificados === 2,
      `${decodificados} decodificação(ões) para dois sons — a mais é uma cópia decodificada por destino, `
        + "todas de pé ao mesmo tempo; a menos, um som que nunca toca",
    );
    const contados = destinos.map((r) => r.bytesDeSomDeCartao).sort((x, y) => x - y);
    confere(
      naContribuicao,
      contados[0] === 0 && contados[1] > 0,
      `o som decodificado ficou contado em mais de um destino, ou em nenhum: ${JSON.stringify(contados)}`,
    );
    const anotadas = destinos.map((r) => r.midiasAnotadas().length).sort((x, y) => x - y);
    confere(
      naContribuicao,
      JSON.stringify(anotadas) === JSON.stringify([0, 2]),
      `o diagnóstico não conta cada som uma vez, no destino que o segura: ${JSON.stringify(anotadas)}`,
    );
    const tocando = AudioC.ultimo?.fontes.filter((f) => f.tocando).length ?? 0;
    confere(
      naContribuicao,
      tocando === 2,
      `${tocando} fonte(s) tocando para dois sons declarados \`tocando\` — uma por destino, ou um som calado pelo outro`,
    );
    const ouviu = dc.ditos.filter((e) => e.nome === "midia" && e.estado === "tocando").length;
    confere(
      naContribuicao,
      ouviu === 2,
      `o MOD ouviu «tocando» ${ouviu} vez(es) de dois sons que declarou uma vez cada`,
    );
    for (const r of destinos) r.soltar();
    const sobrou = AudioC.ultimo?.fontes.filter((f) => f.tocando).length ?? 0;
    confere(naContribuicao, sobrou === 0, `${sobrou} som(ns) da contribuição continuaram depois de os destinos saírem`);
  }

  // **E o som parado de uma contribuição está pronto**, no destino que o
  // tomou, uma vez. Um som de cartão não tem botão: ele só toca quando o MOD
  // declara `tocando`, e até lá quem o diz «pronta» é a anotação do fim de
  // `montarSom` — a aba diria «carregando» para um som que só espera a vez.
  {
    const parado = `${caso} · numa contribuição, um som parado`;
    const AudioP = audioDeMentira();
    const bp = bancada({ audio: AudioP });
    const dp = dono(bp, () => Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 12 }));
    const sonsDaContribuicao = { tomados: false };
    const destinos = ["1", "2"].map(() => {
      const r = new bp.RegiaoDeMod("a/b", dp.api, bp.raiz(), bp.PERFIS.cartao);
      r.sonsDaContribuicao = sonsDaContribuicao;
      r.aplicar([{ forma: "midia", chave: "sino", fonte: "som/a.wav", tocando: false }]);
      return r;
    });
    await assentar();
    const anotadas = destinos.flatMap((r) => r.midiasAnotadas().map((m) => m.situacao));
    confere(
      parado,
      JSON.stringify(anotadas) === JSON.stringify(["pronta"]),
      `o som parado de uma contribuição não ficou anotado «pronta», uma vez, no destino que o tomou: ${JSON.stringify(anotadas)}`,
    );
    for (const r of destinos) r.soltar();
  }
}

async function oSomPedidoSemGestoEDitoRecusado() {
  const caso = "o som pedido sem gesto";
  const Audio = audioDeMentira({ semGesto: true });
  const b = bancada({ audio: Audio });
  const d = dono(b, () =>
    Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 12 }),
  );
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar([{ forma: "midia", chave: "m", fonte: "som/a.wav", tocando: true }]);
  await assentar();
  // O prazo da política é de um segundo e meio; espera-se um pouco mais.
  await new Promise((r) => setTimeout(r, 1700));
  confere(
    caso,
    d.ditos.some((e) => e.nome === "midia" && e.estado === "recusada"),
    `a recusa do navegador não chegou ao MOD: ${JSON.stringify(d.ditos)}`,
  );
  confere(
    caso,
    (Audio.ultimo?.fontes.length ?? 0) === 0,
    "uma fonte foi criada com o áudio da janela parado, e o MOD ouviria «tocando» sobre silêncio",
  );
  const anotada = regiao.midiasAnotadas()[0];
  confere(
    caso,
    anotada?.situacao === "recusada" && /gesto/.test(anotada.motivo),
    `o diagnóstico não disse por que o som não começou: ${JSON.stringify(anotada)}`,
  );
  regiao.soltar();
}

// ---------------------------------------------------------------------------
// 12. O áudio da janela não segura a saída à toa.
// ---------------------------------------------------------------------------

/**
 * Um contexto de tempo real de pé segura a saída de som do sistema aberta,
 * mesmo calado — e no macOS isso impede o repouso por ociosidade (I-2 da
 * revisão ampla do Plano 1D: medido no WKWebView, a asserção do coreaudiod
 * ficou de pé de 5 s a 302 s com o contexto calado, e só `suspend()` a soltou).
 * Um `<audio>` parado não segurava nada.
 *
 * As quatro metades do conserto, cada uma medida aqui:
 * - montar e decodificar não abre a saída: quem decodifica é um contexto fora
 *   do tempo, construído com a assinatura que o `webkitOfflineAudioContext` do
 *   macOS 11 conhece;
 * - o de tempo real nasce no primeiro tocar;
 * - calado pelo prazo, ele é suspenso — e nunca com um som tocando;
 * - ele fecha quando o último tocador sai, e na saída da sessão.
 */
async function oAudioDaJanelaNaoSeguraASaidaAToa() {
  const caso = "o áudio da janela não segura a saída à toa";
  const relogio = relogioDeMentira();
  const Audio = audioDeMentira();
  const b = bancada({ audio: Audio, relogio });
  confere(
    caso,
    Number.isFinite(b.SILENCIO) && b.SILENCIO > 0 && typeof b.encerrarOSomDosMods === "function",
    `a região não tem prazo de silêncio (${b.SILENCIO}) ou não sabe encerrar o som dos MODs na saída da sessão`,
  );
  if (!Number.isFinite(b.SILENCIO) || typeof b.encerrarOSomDosMods !== "function") return;
  const d = dono(b, () => Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 12 }));
  const som = (chave) => [{ forma: "midia", chave, fonte: `som/${chave}.wav` }];

  // Montado e parado: decodificado, com botão, e sem saída aberta.
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar(som("a"));
  await assentar();
  const botao = acharTag(regiao.raiz.children[0], "button");
  confere(caso, regiao.raiz.children[0]?.dataset.estado === "pronta" && Boolean(botao), "o som não ficou de pé, com botão");
  if (!botao) return;
  confere(
    caso,
    Audio.criados === 0,
    `nasceu ${Audio.criados} contexto(s) de tempo real com um som só montado e parado: ele segura a saída `
      + "do sistema, e no macOS o Mac deixa de dormir por ociosidade",
  );
  confere(
    caso,
    Audio.decodificadosNoTempoReal.length === 0 && Audio.ForaDoTempo.criados >= 1 && Audio.decodificados.length === 1,
    "o som não foi decodificado num contexto fora do tempo — o de tempo real abre a saída para decodificar: "
      + `${Audio.decodificadosNoTempoReal.length} no tempo real, ${Audio.ForaDoTempo.criados} fora do tempo`,
  );
  const argumentos = Audio.ForaDoTempo.ultimo?.argumentos ?? [];
  confere(
    caso,
    argumentos.length === 3 && argumentos.every((n) => Number.isInteger(n) && n > 0)
      && argumentos[2] >= 44100 && argumentos[2] <= 96000,
    "o contexto fora do tempo não foi construído com (canais, quadros, taxa) e a taxa entre 44,1 e 96 kHz, a "
      + `única forma que o webkitOfflineAudioContext do macOS 11 aceita: ${JSON.stringify(argumentos)}`,
  );

  // O primeiro tocar cria o contexto, e o fim do som começa o prazo.
  botao.disparar("click");
  await assentar();
  const contexto = Audio.ultimo;
  confere(caso, Audio.criados === 1 && contexto?.fontes[0]?.tocando === true, "o primeiro tocar não criou o contexto e tocou");
  if (!contexto?.fontes[0]) return;
  contexto.fontes[0].onended?.();
  relogio.passar(b.SILENCIO - 1);
  confere(caso, contexto.suspensoes === 0, "o áudio da janela foi suspenso antes do prazo de silêncio");
  relogio.passar(b.SILENCIO);
  await volta();
  confere(
    caso,
    contexto.suspensoes === 1 && contexto.state === "suspended",
    `o som terminou, o prazo de ${b.SILENCIO} ms passou calado, e o áudio da janela continuou de pé `
      + `(${contexto.state}, ${contexto.suspensoes} suspensão(ões)): a saída do sistema fica aberta à toa`,
  );

  // Tocar de novo acorda o mesmo contexto, e não cria outro.
  botao.disparar("click");
  await assentar();
  confere(
    caso,
    Audio.criados === 1 && contexto.state === "running" && contexto.fontes[1]?.tocando === true,
    `tocar depois da suspensão não acordou o mesmo contexto: ${Audio.criados} criado(s), ${contexto.state}`,
  );

  // Com um som tocando, o prazo não suspende nada — nem quando outro para.
  const outra = new b.RegiaoDeMod("a/c", d.api, b.raiz());
  outra.aplicar(som("b"));
  await assentar();
  const outroBotao = acharTag(outra.raiz.children[0], "button");
  outroBotao?.disparar("click");
  await assentar();
  confere(caso, contexto.fontes[2]?.tocando === true, "o segundo som não tocou no mesmo áudio da janela");
  outroBotao?.disparar("click");
  const suspensoesAntes = contexto.suspensoes;
  relogio.passar(b.SILENCIO);
  confere(
    caso,
    contexto.suspensoes === suspensoesAntes && contexto.state === "running",
    "o áudio da janela foi suspenso com um som tocando, porque outro som parou: "
      + `${contexto.suspensoes - suspensoesAntes} suspensão(ões), ${contexto.state}`,
  );

  // Pausado o último som, o prazo suspende de novo — e quem aperta TOCAR no
  // meio da suspensão espera ela terminar e acorda o áudio. Sem a espera, a
  // fonte começava num contexto que ainda dizia «running» e se suspendia logo
  // depois: o MOD ouvia «tocando», e ninguém ouvia nada.
  botao.disparar("click");
  relogio.passar(b.SILENCIO);
  confere(
    caso,
    contexto.suspensoes === suspensoesAntes + 1,
    `o último som foi pausado, o prazo passou, e o áudio da janela não foi suspenso: ${contexto.suspensoes - suspensoesAntes}`,
  );
  const acordadoAntes = contexto.acordar;
  botao.disparar("click");
  await assentar();
  confere(
    caso,
    contexto.state === "running" && contexto.acordar > acordadoAntes && contexto.fontes.at(-1)?.tocando === true,
    "tocar no meio de uma suspensão começou o som sem acordar o áudio da janela, que se suspendeu por baixo "
      + `dele: ${contexto.state}, ${contexto.acordar - acordadoAntes} pedido(s) de acordar`,
  );
  botao.disparar("click");
  relogio.passar(b.SILENCIO);
  await volta();
  confere(caso, contexto.state === "suspended", `pausado de novo, o áudio da janela não voltou a ser suspenso: ${contexto.state}`);

  // Sai o último tocador: o contexto fecha — e não antes.
  regiao.soltar();
  confere(caso, contexto.fechamentos === 0, "o áudio da janela fechou com um tocador ainda de pé");
  outra.soltar();
  confere(
    caso,
    contexto.fechamentos === 1,
    `o último tocador saiu e o áudio da janela não fechou: ${contexto.fechamentos} fechamento(s)`,
  );

  // A saída da sessão cala o som que toca e fecha o contexto na hora, sem
  // esperar cada instância confirmar que parou.
  const depois = new b.RegiaoDeMod("a/d", d.api, b.raiz());
  depois.aplicar([{ ...som("c")[0], tocando: true }]);
  await assentar();
  const segundo = Audio.ultimo;
  confere(caso, Audio.criados === 2 && segundo?.fontes[0]?.tocando === true, "depois do fechamento, tocar não criou um contexto novo");
  b.encerrarOSomDosMods();
  confere(
    caso,
    segundo?.fontes[0]?.parou === true && segundo?.fechamentos === 1,
    "a saída da sessão não calou o som que tocava ou não fechou o áudio da janela: "
      + `parou=${segundo?.fontes[0]?.parou}, ${segundo?.fechamentos} fechamento(s)`,
  );

  // Tocado de novo depois da saída da sessão, o som toca no contexto novo — e
  // pelo ganho dele: o ganho de antes está ligado a um contexto fechado, e uma
  // fonte ligada a ele ninguém ouve (S-m6 da revisão do Lote Som).
  acharTag(depois.raiz.children[0], "button")?.disparar("click");
  await assentar();
  const terceiro = Audio.ultimo;
  const fonteNova = terceiro?.fontes[0];
  confere(
    caso,
    Audio.criados === 3 && fonteNova?.tocando === true && terceiro.ganhos.includes(fonteNova.ligado)
      && fonteNova.ligado?.ligado === terceiro.destination,
    "tocado de novo depois da saída da sessão, o som não tocou pelo ganho do contexto novo — o de antes está ligado "
      + `a um contexto fechado, e ninguém ouviria nada: ${Audio.criados} contexto(s), fonte ligada a um ganho `
      + `${terceiro?.ganhos.includes(fonteNova?.ligado) ? "do contexto novo" : "de outro contexto"}`,
  );
  depois.soltar();
  confere(caso, segundo?.fechamentos === 1, `o áudio da janela foi fechado ${segundo?.fechamentos} vezes`);
}

/**
 * **A suspensão é do produto, e a recusa não culpa o gesto** (S-m4 da revisão
 * do Lote Som). Depois de o produto suspender o áudio calado, o som que o MOD
 * declara de novo — a trilha da MESA, pelo m-4 — espera a saída acordar. Ela
 * não depende de gesto nenhum: o áudio já tocou nesta janela. Esperando só o
 * prazo da política, um fone Bluetooth que leva dois segundos para acordar
 * fazia a trilha ser recusada com «sem um gesto de quem usa», um motivo falso
 * — e, recusada, ela não voltava mais pelo redesenho.
 */
async function aSuspensaoDoProdutoEsperaASaidaAcordar() {
  const caso = "a suspensão do produto";
  const midiaDeSom = () => Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 12 });
  const trilha = [{ forma: "midia", chave: "trilha", fonte: "som/trilha.wav", tocando: true }];
  /** Toca, termina e deixa o prazo de silêncio suspender o áudio: o que o produto faz entre duas trilhas. */
  const suspensaPeloProduto = async (Audio) => {
    const relogio = relogioDeMentira();
    const b = bancada({ audio: Audio, relogio });
    const d = dono(b, midiaDeSom);
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar(trilha);
    await assentar();
    const contexto = Audio.ultimo;
    contexto?.fontes[0]?.onended?.();
    relogio.passar(b.SILENCIO);
    await volta();
    return { relogio, d, regiao, contexto };
  };

  // A saída leva dois segundos para acordar: mais que o prazo da política, e a
  // trilha toca mesmo assim.
  {
    let acordar = null;
    class Lento extends audioDeMentira() {
      resume() {
        this.acordar += 1;
        return new Promise((pronto) => {
          acordar = () => {
            this.state = "running";
            pronto();
          };
        });
      }
    }
    const { relogio, d, regiao, contexto } = await suspensaPeloProduto(Lento);
    confere(caso, contexto?.state === "suspended", `o produto não suspendeu o áudio calado: ${contexto?.state}`);
    regiao.aplicar(trilha);
    await assentar();
    relogio.passar(2000);
    await assentar();
    acordar?.();
    await assentar();
    const recusas = d.ditos.filter((e) => e.nome === "midia" && e.estado === "recusada").map((e) => e.porque);
    confere(
      caso,
      contexto?.fontes[1]?.tocando === true && recusas.length === 0,
      "o produto suspendeu o áudio calado, a saída levou 2 s para acordar — um fone Bluetooth —, e a trilha "
        + `declarada foi recusada em vez de esperar: ${contexto?.fontes.length} fonte(s), recusas ${JSON.stringify(recusas)}`,
    );
    regiao.soltar();
  }

  // A saída não acorda nunca: a recusa diz isso, e não que faltou um gesto.
  {
    class Mudo extends audioDeMentira() {
      resume() {
        this.acordar += 1;
        return new Promise(() => {});
      }
    }
    const { relogio, d, regiao, contexto } = await suspensaPeloProduto(Mudo);
    regiao.aplicar(trilha);
    await assentar();
    relogio.passar(60_000);
    await assentar();
    const dita = d.ditos.find((e) => e.nome === "midia" && e.estado === "recusada");
    const anotada = regiao.midiasAnotadas()[0];
    const registrada = d.anotadas.find((t) => t.includes("«trilha»")) ?? "";
    confere(
      caso,
      (contexto?.fontes.length ?? 0) === 1
        && [dita?.porque, anotada?.motivo, registrada].every((t) => /não acordou a tempo/.test(t ?? "") && !/gesto/.test(t ?? "")),
      "a saída que o produto suspendeu não acordou, e a recusa culpou o gesto de quem usa ou não disse que a saída "
        + `não acordou a tempo: ao MOD ${JSON.stringify(dita?.porque)}, ao diagnóstico ${JSON.stringify(anotada?.motivo)}, `
        + `ao registro ${JSON.stringify(registrada)}`,
    );
    regiao.soltar();
  }

  // O navegador recusa acordá-la, na hora: não houve prazo vencido, e a frase
  // não diz «a tempo» — diz que ela não acordou, com o que o navegador
  // respondeu (D-m2 da revisão do Lote Diagnóstico da correção ampla do Plano
  // 1D).
  {
    class Recusa extends audioDeMentira() {
      resume() {
        this.acordar += 1;
        return Promise.reject(new Error("NotAllowedError: a saída de som não abriu"));
      }
    }
    const { d, regiao, contexto } = await suspensaPeloProduto(Recusa);
    regiao.aplicar(trilha);
    await assentar();
    const dita = d.ditos.find((e) => e.nome === "midia" && e.estado === "recusada");
    const anotada = regiao.midiasAnotadas()[0];
    const registrada = d.anotadas.find((t) => t.includes("«trilha»")) ?? "";
    confere(
      caso,
      (contexto?.fontes.length ?? 0) === 1
        && [dita?.porque, anotada?.motivo, registrada].every((t) => /a saída de som, que o produto suspendeu no silêncio, não acordou/.test(t ?? "")
          && /NotAllowedError: a saída de som não abriu/.test(t ?? "")
          && !/a tempo|gesto/.test(t ?? "")),
      "o navegador recusou acordar a saída que o produto suspendeu, e a recusa disse que ela não acordou «a tempo» — "
        + "um prazo que não venceu —, culpou o gesto, ou calou o que o navegador respondeu: ao MOD "
        + `${JSON.stringify(dita?.porque)}, ao diagnóstico ${JSON.stringify(anotada?.motivo)}, ao registro ${JSON.stringify(registrada)}`,
    );
    regiao.soltar();
  }

  // A marca vale até o áudio voltar a ligar. Acordado, e parado depois por
  // outro motivo — o navegador, e não o silêncio do produto —, ele volta a
  // esperar só o prazo da política: a frase de quem suspendeu seria falsa.
  {
    class Alternado extends audioDeMentira() {
      resume() {
        this.acordar += 1;
        if (this.mudo) return new Promise(() => {});
        this.state = "running";
        return Promise.resolve();
      }
    }
    const { relogio, d, regiao, contexto } = await suspensaPeloProduto(Alternado);
    regiao.aplicar(trilha);
    await assentar();
    confere(caso, contexto?.fontes[1]?.tocando === true, "o áudio que o produto suspendeu não acordou para a trilha declarada");
    contexto?.fontes[1]?.onended?.();
    if (contexto) {
      contexto.state = "suspended";
      contexto.mudo = true;
    }
    regiao.aplicar(trilha);
    await assentar();
    relogio.passar(2000);
    await assentar();
    const dita = d.ditos.find((e) => e.nome === "midia" && e.estado === "recusada");
    confere(
      caso,
      /gesto/.test(dita?.porque ?? ""),
      "o áudio acordou depois da suspensão do produto, o navegador o parou de novo, e o pedido seguinte continuou "
        + `tratado como se o produto o tivesse suspendido: ${JSON.stringify(dita?.porque)}`,
    );
    regiao.soltar();
  }
}

// ---------------------------------------------------------------------------
// 13. Os bytes do som chegam pelos dois caminhos do `ipc`.
// ---------------------------------------------------------------------------

/**
 * `som_do_mod` responde com bytes crus, e eles chegam de dois jeitos: pelo
 * protocolo `ipc:` — o caminho normal, que a CSP abre —, como `ArrayBuffer`; e
 * pelo recuo do `postMessage`, como lista de números. `bufferDoSom` aceita os
 * dois, e uma vista tipada.
 *
 * O `ArrayBuffer` é criado **no reino da janela** (`b.Uint8Array`): um de fora
 * da vm não passa no `instanceof ArrayBuffer`, e a prova mediria o ramo errado.
 * Sem esta prova, o ramo do `ArrayBuffer` só era medido na bancada Playwright,
 * que roda num job manual — e, se ele regredisse, todo som do pacote se calava
 * com «o som não chegou em bytes» e todos os portões ficavam verdes (I-4 da
 * revisão ampla do Plano 1D).
 */
async function osBytesDoSomChegamPelosDoisCaminhosDoIpc() {
  const caso = "os bytes do som pelo ipc";
  const formas = {
    "um ArrayBuffer, pelo protocolo ipc:": (b) => new b.Uint8Array(BYTES_DE_SOM).buffer,
    "uma lista de números, pelo recuo do postMessage": () => [...BYTES_DE_SOM],
    "uma vista tipada": (b) => new b.Uint8Array(BYTES_DE_SOM),
  };
  for (const [forma, entregar] of Object.entries(formas)) {
    const Audio = audioDeMentira();
    const b = bancada({ audio: Audio });
    const d = dono(b, () => Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 12 }));
    d.api.bytesDoSom = () => Promise.resolve(entregar(b));
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([{ forma: "midia", chave: "m", fonte: "som/a.wav" }]);
    await assentar();
    const figura = regiao.raiz.children[0];
    const falhou = d.ditos.find((e) => e.nome === "midia" && e.estado === "falhou");
    confere(
      `${caso} · ${forma}`,
      figura?.dataset.estado === "pronta" && Audio.decodificados.join() === String(BYTES_DE_SOM.length),
      `o som do pacote entregue como ${forma} não chegou ao WebAudio: ${figura?.dataset.estado}, `
        + `${falhou?.porque ?? "sem motivo dito"}, decodificados ${JSON.stringify(Audio.decodificados)}`,
    );
    regiao.soltar();
  }
}

// ---------------------------------------------------------------------------
// 14. As proteções do tocador e da montagem do som.
// ---------------------------------------------------------------------------

/**
 * Promessas do doc que podiam regredir caladas (m-5 da revisão ampla do Plano
 * 1D): cada uma é uma linha que nenhuma prova via.
 *
 * - **continuar de onde parou**: pausar lembra o ponto, e tocar de novo
 *   começa dali — ou do começo, se o ponto passou do fim;
 * - **o pedido superado não toca**: um `tocando` desligado enquanto o áudio
 *   da janela ainda não ligou não começa quando ele liga;
 * - **sair no meio do caminho**: o MOD tira o som da tela enquanto os bytes
 *   vêm, ou enquanto eles decodificam, e nada segue — nem decodificação à
 *   toa, nem bytes fantasmas no bolso, nem anotação sem nó;
 * - **tirar a mídia avisa**: a gestão de MODs é avisada de que a soma mudou.
 */
async function asProtecoesDoTocadorEDaMontagemDoSom() {
  const caso = "as proteções do som";
  const midiaDeSom = () => Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 12 });

  // Continuar de onde parou — e do começo, quando o ponto passou do fim.
  {
    const Audio = audioDeMentira();
    const b = bancada({ audio: Audio });
    const d = dono(b, midiaDeSom);
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([{ forma: "midia", chave: "m", fonte: "som/a.wav" }]);
    await assentar();
    const botao = acharTag(regiao.raiz.children[0], "button");
    botao?.disparar("click");
    await assentar();
    const contexto = Audio.ultimo;
    if (contexto) contexto.currentTime = 0.4;
    botao?.disparar("click");
    botao?.disparar("click");
    await assentar();
    const de = contexto?.fontes[1]?.de;
    confere(
      caso,
      typeof de === "number" && Math.abs(de - 0.4) < 1e-9,
      `pausar e continuar recomeçou o som de ${de}, e não de 0.4, onde ele parou`,
    );
    // O som de mentira dura um segundo: parado em 5,4 s, ele passou do fim.
    if (contexto) contexto.currentTime = 5.4;
    botao?.disparar("click");
    botao?.disparar("click");
    await assentar();
    const doComeco = contexto?.fontes[2]?.de;
    confere(caso, doComeco === 0, `parado depois do fim do som, tocar de novo começou de ${doComeco}, e não do começo`);
    regiao.soltar();
  }

  // O pedido superado não toca.
  {
    let ligar = null;
    class Contexto extends audioDeMentira({ semGesto: true }) {
      resume() {
        this.acordar += 1;
        return new Promise((pronto) => {
          ligar = () => {
            this.state = "running";
            pronto();
          };
        });
      }
    }
    const b = bancada({ audio: Contexto });
    const d = dono(b, midiaDeSom);
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([{ forma: "midia", chave: "m", fonte: "som/a.wav", tocando: true }]);
    await assentar();
    confere(caso, typeof ligar === "function", "o `tocando` declarado não pediu ao áudio da janela que ligasse");
    regiao.aplicar([{ forma: "midia", chave: "m", fonte: "som/a.wav", tocando: false }]);
    ligar?.();
    await assentar();
    const tocando = Contexto.ultimo?.fontes.filter((f) => f.tocando).length ?? 0;
    const ouviu = d.ditos.filter((e) => e.nome === "midia" && e.estado === "tocando").length;
    confere(
      caso,
      tocando === 0 && ouviu === 0,
      "o MOD desligou o `tocando` antes de o áudio da janela ligar, e o som começou mesmo assim: "
        + `${tocando} fonte(s), «tocando» dito ${ouviu} vez(es)`,
    );
    regiao.soltar();
  }

  // Sair durante os bytes, ou durante a decodificação.
  for (const quando of ["os bytes", "a decodificação"]) {
    const Audio = audioDeMentira({ segurarADecodificacao: quando === "a decodificação" });
    const b = bancada({ audio: Audio });
    const d = dono(b, midiaDeSom);
    let soltarOsBytes = null;
    if (quando === "os bytes") {
      d.api.bytesDoSom = () => new Promise((pronto) => {
        soltarOsBytes = () => pronto(new b.Uint8Array(BYTES_DE_SOM).buffer);
      });
    }
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([{ forma: "midia", chave: "m", fonte: "som/a.wav", tocando: true }]);
    await assentar();
    // O MOD tira o som da tela no meio do caminho, e o caminho termina depois.
    regiao.aplicar([]);
    soltarOsBytes?.();
    for (const presa of Audio.decodificacoesPresas) {
      presa.pronto({ duration: 1, length: 48000, numberOfChannels: 2, sampleRate: 48000 });
    }
    await assentar();
    const sobrou = {
      decodificados: Audio.decodificados.length,
      contaDoSom: regiao.bytesDeSomDecodificado,
      anotadas: regiao.midiasAnotadas().map((m) => m.situacao),
      fontes: Audio.ultimo?.fontes.length ?? 0,
    };
    const esperado = { decodificados: quando === "os bytes" ? 0 : 1, contaDoSom: 0, anotadas: [], fontes: 0 };
    confere(
      `${caso} · sair durante ${quando}`,
      JSON.stringify(sobrou) === JSON.stringify(esperado),
      `o MOD tirou o som da tela durante ${quando}, e a montagem seguiu: ${JSON.stringify(sobrou)}, e não `
        + `${JSON.stringify(esperado)} — uma decodificação à toa, bytes fantasmas no bolso, ou uma anotação sem nó`,
    );
    regiao.soltar();
  }

  // Sem o áudio de tempo real na hora de tocar, a recusa diz isso — e não
  // que faltou um gesto de quem usa.
  {
    const b = bancada();
    const d = dono(b, midiaDeSom);
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([{ forma: "midia", chave: "m", fonte: "som/a.wav" }]);
    await assentar();
    b.semAudioDeTempoReal();
    acharTag(regiao.raiz.children[0], "button")?.disparar("click");
    await assentar();
    const anotada = regiao.midiasAnotadas()[0];
    confere(
      caso,
      anotada?.situacao === "recusada" && /WebAudio/.test(anotada.motivo) && !/gesto/.test(anotada.motivo),
      `tocar sem o áudio de tempo real da janela não foi recusado com o motivo certo: ${JSON.stringify(anotada)}`,
    );
    regiao.soltar();
  }

  // Tirar a mídia avisa a gestão.
  {
    const b = bancada();
    const d = dono(b, () => Promise.resolve({ uri: "data:image/png;base64,AA", papel: "imagem", bytes: 12 }));
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.declararCartoes({ 7: [{ forma: "midia", chave: "m", fonte: "img/a.png" }] });
    await assentar();
    const antes = d.mudancas();
    regiao.declararCartoes({});
    confere(
      caso,
      d.mudancas() > antes && regiao.midiasAnotadas().length === 0,
      "a mídia do cartão saiu e a gestão de MODs não foi avisada: a aba DIAGNÓSTICO seguiria contando uma "
        + `mídia que não existe mais (${d.mudancas() - antes} aviso(s))`,
    );
    regiao.soltar();
  }
}

// ---------------------------------------------------------------------------
// 15. O som que terminou volta a tocar com o `tocando` declarado.
// ---------------------------------------------------------------------------

/**
 * Com o `<audio>`, até a 0.15.0, um `tocando: true` sobre um som que já tinha
 * **terminado** o tocava de novo no próximo `aplicar` — o `play()` de um
 * elemento no fim recomeça do começo. A MESA publicada conta com isso: as
 * trilhas dela têm 30 s, ela não escuta «terminou», e redesenha declarando a
 * trilha `tocando` enquanto o mestre a quer tocando; a trilha volta a cada
 * redesenho. Com o WebAudio, a declaração passou a valer só na mudança, e a
 * trilha se calava em 30 s (m-4 da revisão ampla do Plano 1D). O comportamento
 * de antes volta para os MODs de hoje; a repetição explícita é da API 6.
 *
 * O que **não** volta: um som que a pessoa pausou, e um que o navegador
 * recusou, não são religados pelo redesenho — a declaração continua valendo
 * na mudança para eles, e o botão do produto vale entre uma mudança e outra.
 */
async function oSomQueTerminouVoltaComOTocandoDeclarado() {
  const caso = "o som que terminou volta com o tocando declarado";
  const midiaDeSom = () => Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 12 });
  const trilha = [{ forma: "midia", chave: "trilha", fonte: "som/trilha.wav", tocando: true }];
  const tocou = (d) => d.ditos.filter((e) => e.nome === "midia" && e.estado === "tocando").length;

  const Audio = audioDeMentira();
  const b = bancada({ audio: Audio });
  const d = dono(b, midiaDeSom);
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar(trilha);
  await assentar();
  const contexto = Audio.ultimo;
  confere(caso, contexto?.fontes[0]?.tocando === true, "a trilha declarada `tocando` não tocou");
  if (!contexto?.fontes[0]) return;

  // Ela termina, e o MOD redesenha declarando-a `tocando`, como antes.
  contexto.fontes[0].onended?.();
  regiao.aplicar(trilha);
  await assentar();
  confere(
    caso,
    contexto.fontes[1]?.tocando === true && contexto.fontes[1].de === 0 && tocou(d) === 2,
    "a trilha terminou, o MOD redesenhou com `tocando: true`, e ela não voltou a tocar do começo — a da MESA "
      + `se cala em 30 s: ${contexto.fontes.length} fonte(s), «tocando» dito ${tocou(d)} vez(es)`,
  );

  // Tocando, um redesenho não a recomeça.
  regiao.aplicar(trilha);
  await assentar();
  confere(
    caso,
    contexto.fontes.length === 2 && contexto.fontes[1].tocando === true,
    `um redesenho com a trilha tocando a recomeçou: ${contexto.fontes.length} fonte(s)`,
  );

  // A pessoa pausa: o redesenho não a religa.
  const botao = acharTag(regiao.raiz.children[0], "button");
  botao?.disparar("click");
  regiao.aplicar(trilha);
  await assentar();
  confere(
    caso,
    contexto.fontes.length === 2 && contexto.fontes[1].parou === true,
    `a pessoa pausou a trilha, e o redesenho do MOD a religou: ${contexto.fontes.length} fonte(s)`,
  );
  regiao.soltar();

  // O fim que a pessoa já desfez não é mais fim: a trilha termina, a pessoa a
  // toca pelo botão e a pausa, e o redesenho do MOD não a religa — o último
  // aviso dela é «pausada», e não «terminou» (S-m1 da revisão do Lote Som).
  {
    const AudioP = audioDeMentira();
    const bp = bancada({ audio: AudioP });
    const dp = dono(bp, midiaDeSom);
    const pausada = new bp.RegiaoDeMod("a/b", dp.api, bp.raiz());
    pausada.aplicar(trilha);
    await assentar();
    const contextoP = AudioP.ultimo;
    contextoP?.fontes[0]?.onended?.();
    const botaoP = acharTag(pausada.raiz.children[0], "button");
    botaoP?.disparar("click");
    await assentar();
    botaoP?.disparar("click");
    const antes = contextoP?.fontes.length ?? 0;
    pausada.aplicar(trilha);
    await assentar();
    confere(
      caso,
      antes === 2 && (contextoP?.fontes.length ?? 0) === 2,
      "a trilha terminou, a pessoa a tocou pelo botão e a pausou, e o redesenho do MOD com `tocando: true` a "
        + `religou como se ela tivesse terminado: ${(contextoP?.fontes.length ?? 0) - antes} fonte(s) nova(s)`,
    );
    pausada.soltar();
  }

  // Um som que o navegador recusou também não é tentado de novo a cada
  // redesenho: a recusa é dita uma vez, e quem a desfaz é a mudança.
  const AudioR = audioDeMentira({ recusaNaHora: true });
  const br = bancada({ audio: AudioR });
  const dr = dono(br, midiaDeSom);
  const recusado = new br.RegiaoDeMod("a/b", dr.api, br.raiz());
  recusado.aplicar(trilha);
  await assentar();
  const pedidos = AudioR.ultimo?.acordar ?? 0;
  confere(caso, dr.ditos.some((e) => e.nome === "midia" && e.estado === "recusada"), "o som que o navegador recusou não foi dito recusado");
  recusado.aplicar(trilha);
  await assentar();
  confere(
    caso,
    (AudioR.ultimo?.acordar ?? 0) === pedidos,
    `o som que o navegador recusou foi pedido de novo pelo redesenho: ${(AudioR.ultimo?.acordar ?? 0) - pedidos} pedido(s) a mais`,
  );
  recusado.soltar();

  // E o fim vale uma volta só: com o áudio da janela suspenso pelo silêncio,
  // o som que volta espera ele acordar, e um redesenho nesse meio tempo não
  // pede de novo — senão cada redesenho da espera seria um pedido de acordar.
  const relogio = relogioDeMentira();
  let acordar = null;
  class Lento extends audioDeMentira() {
    resume() {
      this.acordar += 1;
      return new Promise((pronto) => {
        acordar = () => {
          this.state = "running";
          pronto();
        };
      });
    }
  }
  const bl = bancada({ audio: Lento, relogio });
  const dl = dono(bl, midiaDeSom);
  const lenta = new bl.RegiaoDeMod("a/b", dl.api, bl.raiz());
  lenta.aplicar(trilha);
  await assentar();
  const contextoLento = Lento.ultimo;
  contextoLento?.fontes[0]?.onended?.();
  relogio.passar(bl.SILENCIO);
  await volta();
  lenta.aplicar(trilha);
  await assentar();
  lenta.aplicar(trilha);
  await assentar();
  const pedidosDeAcordar = contextoLento?.acordar ?? 0;
  acordar?.();
  await assentar();
  confere(
    caso,
    contextoLento?.state === "running" && pedidosDeAcordar === 1 && contextoLento?.fontes[1]?.tocando === true,
    "o som que terminou foi pedido de novo a cada redesenho enquanto esperava o áudio da janela acordar: "
      + `${pedidosDeAcordar} pedido(s) de acordar, ${contextoLento?.state}`,
  );
  lenta.soltar();
}

// ---------------------------------------------------------------------------
// 16. O som que sai da tela para, como o `<audio>` parava.
// ---------------------------------------------------------------------------

/**
 * Pelos «removing steps» do HTML, um `<audio>` pausa sozinho quando sai do
 * documento; uma fonte de WebAudio não, porque está ligada à saída de som. O
 * descarte cala o som (`sairDuranteAReproducaoPara`), mas o produto também tira
 * nós da tela **sem** descartá-los, e nos cinco caminhos que a revisão ampla do
 * Plano 1D mediu (I-1) o som seguia até o fim, sem controle à vista e sem
 * `pausada` ao MOD: a página fechada, a contribuição que perde a disputa, o
 * cartão da API 3 sob «o SEELE desenha», o destino de quem saiu da sala, e a
 * página fechada que recebe um som novo `tocando`.
 *
 * Esta prova mede `RegiaoDeMod#calarSonsForaDaTela` — quem a chama nos
 * desenhos do produto é medido em `contribuicoes-e-camadas.cjs` (a página
 * fechada e a varredura de `base.js`) e, no Chromium, em
 * `diagnostico-de-mods.cjs`.
 */
async function oSomQueSaiDaTelaParaComoOAudioParava() {
  const caso = "o som que sai da tela para";
  const midiaDeSom = () => Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 12 });
  const ditos = (d, chave, estado) => d.ditos.filter((e) => e.nome === "midia" && e.chave === chave && e.estado === estado);
  const trilha = { forma: "midia", chave: "trilha", fonte: "som/trilha.wav", tocando: true };
  const nova = { forma: "midia", chave: "nova", fonte: "som/nova.wav", tocando: true };

  // A página fechada: o nó sai do documento, e o renderer fica para reabrir.
  {
    const Audio = audioDeMentira();
    const b = bancada({ audio: Audio });
    const d = dono(b, midiaDeSom);
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([trilha]);
    await assentar();
    const contexto = Audio.ultimo;
    confere(caso, contexto?.fontes[0]?.tocando === true, "a trilha declarada `tocando` não tocou");
    if (!contexto?.fontes[0]) return;
    // Na tela, calar não cala nada.
    regiao.calarSonsForaDaTela();
    confere(caso, contexto.fontes[0].tocando === true, "a trilha foi calada com a página aberta, na tela");

    regiao.raiz.remove();
    regiao.calarSonsForaDaTela();
    confere(
      caso,
      contexto.fontes[0].parou === true && ditos(d, "trilha", "pausada").length === 1,
      "a página fechou e a trilha seguiu tocando fora da tela, sem controle à vista: "
        + `parou=${contexto.fontes[0].parou}, «pausada» dita ${ditos(d, "trilha", "pausada").length} vez(es) ao MOD`,
    );
    confere(
      caso,
      regiao.midiasAnotadas()[0]?.situacao === "pronta",
      `a trilha calada pela página fechada não ficou anotada «pronta»: ${JSON.stringify(regiao.midiasAnotadas())}`,
    );

    // Fechada, o redesenho do MOD com `tocando: true` não a religa.
    regiao.aplicar([trilha]);
    await assentar();
    confere(
      caso,
      contexto.fontes.length === 1,
      `com a página fechada, um redesenho do MOD com \`tocando: true\` religou a trilha: ${contexto.fontes.length} fonte(s)`,
    );

    // Fechada, um som novo declarado `tocando` não toca: é recusado, e o
    // motivo vai ao MOD, ao registro e ao diagnóstico.
    regiao.aplicar([trilha, nova]);
    await assentar();
    const recusa = ditos(d, "nova", "recusada")[0];
    confere(
      caso,
      contexto.fontes.length === 1 && /tela/.test(recusa?.porque ?? ""),
      "um som novo declarado `tocando` numa página fechada tocou, ou foi recusado sem dizer por quê ao MOD: "
        + `${contexto.fontes.length} fonte(s), ${JSON.stringify(recusa)}`,
    );
    confere(
      caso,
      d.anotadas.some((t) => t.includes("«nova»") && /tela/.test(t)),
      `a recusa do som da página fechada não chegou ao registro: ${JSON.stringify(d.anotadas)}`,
    );
    confere(
      caso,
      regiao.midiasAnotadas().some((m) => m.situacao === "recusada" && /tela/.test(m.motivo)),
      `a recusa do som da página fechada não chegou ao diagnóstico: ${JSON.stringify(regiao.midiasAnotadas())}`,
    );

    // Reaberta, o redesenho também não religa nada sozinho: quem religa é o
    // botão do produto, ou o MOD mudando o `tocando`.
    b.doc.body.append(regiao.raiz);
    regiao.aplicar([trilha, nova]);
    await assentar();
    confere(caso, contexto.fontes.length === 1, `a página reabriu e um redesenho religou um som sozinho: ${contexto.fontes.length} fonte(s)`);
    acharTag(regiao.raiz.children[0], "button")?.disparar("click");
    await assentar();
    confere(caso, contexto.fontes[1]?.tocando === true, "reaberta a página, o botão do produto não tocou a trilha");
    regiao.soltar();
  }

  // O pedido que ainda esperava o áudio da janela ligar quando a página fechou:
  // calar o cancela, e o cancelamento é dito — recusado, com o motivo, ao MOD,
  // ao registro e ao diagnóstico —, e não desfeito em silêncio (S-m5 da
  // revisão do Lote Som). Sem isto, o MOD declarou `tocando` e não ouviu
  // «tocando», nem «pausada», nem «recusada».
  {
    const relogio = relogioDeMentira();
    let acordar = null;
    class Lento extends audioDeMentira({ semGesto: true }) {
      resume() {
        this.acordar += 1;
        return new Promise((pronto) => {
          acordar = () => {
            this.state = "running";
            pronto();
          };
        });
      }
    }
    const b = bancada({ audio: Lento, relogio });
    const d = dono(b, midiaDeSom);
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([trilha]);
    await assentar();
    confere(caso, typeof acordar === "function", "a trilha declarada `tocando` não pediu ao áudio da janela que ligasse");
    regiao.raiz.remove();
    regiao.calarSonsForaDaTela();
    acordar?.();
    await assentar();
    const recusa = ditos(d, "trilha", "recusada")[0];
    confere(
      caso,
      (Lento.ultimo?.fontes.length ?? 0) === 0 && /tela/.test(recusa?.porque ?? "")
        && regiao.midiasAnotadas().some((m) => m.situacao === "recusada" && /tela/.test(m.motivo))
        && d.anotadas.some((t) => t.includes("«trilha»") && /tela/.test(t)),
      "a página fechou com a trilha esperando o áudio da janela ligar, e o pedido foi desfeito em silêncio, ou tocou: "
        + `${Lento.ultimo?.fontes.length} fonte(s), ao MOD ${JSON.stringify(recusa)}, ao diagnóstico `
        + `${JSON.stringify(regiao.midiasAnotadas())}, ao registro ${JSON.stringify(d.anotadas)}`,
    );
    regiao.soltar();

    // Um pedido que o próprio MOD já tinha desligado não é de ninguém: a
    // página que fecha depois não o recusa.
    const b2 = bancada({ audio: Lento, relogio });
    const d2 = dono(b2, midiaDeSom);
    const desligado = new b2.RegiaoDeMod("a/b", d2.api, b2.raiz());
    desligado.aplicar([trilha]);
    await assentar();
    desligado.aplicar([{ ...trilha, tocando: false }]);
    desligado.raiz.remove();
    desligado.calarSonsForaDaTela();
    await assentar();
    confere(
      caso,
      ditos(d2, "trilha", "recusada").length === 0,
      "o MOD desligou o `tocando` antes de o áudio ligar, a página fechou depois, e o pedido que já não era de "
        + `ninguém foi recusado: ${JSON.stringify(ditos(d2, "trilha", "recusada"))}`,
    );
    desligado.soltar();
  }

  // O som que terminou na tela não volta pelo redesenho com a página fechada
  // — nem recusado: o fim que a declaração religaria (m-4) é o da tela.
  {
    const Audio = audioDeMentira();
    const b = bancada({ audio: Audio });
    const d = dono(b, midiaDeSom);
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    regiao.aplicar([trilha]);
    await assentar();
    Audio.ultimo?.fontes[0]?.onended?.();
    regiao.raiz.remove();
    regiao.calarSonsForaDaTela();
    regiao.aplicar([trilha]);
    await assentar();
    confere(
      caso,
      (Audio.ultimo?.fontes.length ?? 0) === 1 && ditos(d, "trilha", "recusada").length === 0,
      "a trilha terminou, a página fechou, e o redesenho a religou ou a recusou: "
        + `${Audio.ultimo?.fontes.length} fonte(s), ${ditos(d, "trilha", "recusada").length} recusa(s)`,
    );
    regiao.soltar();
  }

  // O cartão da API 3: quando o SEELE desenha o cartão, o som do cartão para
  // — e o da região, não. E o cartão que sai da lista, também.
  {
    const Audio = audioDeMentira();
    const b = bancada({ audio: Audio });
    const d = dono(b, midiaDeSom);
    const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
    const cartoes = {
      7: [{ forma: "midia", chave: "cartao-7", fonte: "som/c7.wav", tocando: true }],
      8: [{ forma: "midia", chave: "cartao-8", fonte: "som/c8.wav", tocando: true }],
    };
    regiao.aplicar([{ forma: "midia", chave: "faixa", fonte: "som/f.wav", tocando: true }]);
    regiao.declararCartoes(cartoes);
    b.doc.body.append(regiao.cartaoDe(7), regiao.cartaoDe(8));
    await assentar();
    const tocando = () => (Audio.ultimo?.fontes ?? []).filter((f) => f.tocando).length;
    confere(caso, tocando() === 3, `a faixa e os dois cartões não tocaram: ${tocando()} fonte(s)`);

    // A pessoa 8 sai da sala: o cartão dela sai da lista, e o MOD vale.
    regiao.cartaoDe(8).remove();
    regiao.calarSonsForaDaTela(true);
    confere(
      caso,
      ditos(d, "cartao-8", "pausada").length === 1 && tocando() === 2,
      `o cartão de quem saiu da lista seguiu tocando: ${tocando()} fonte(s), «pausada» dita `
        + `${ditos(d, "cartao-8", "pausada").length} vez(es)`,
    );
    // «O SEELE desenha» o cartão: os cartões deste MOD deixam de valer.
    regiao.calarSonsForaDaTela(false);
    confere(
      caso,
      ditos(d, "cartao-7", "pausada").length === 1 && ditos(d, "faixa", "pausada").length === 0 && tocando() === 1,
      "com «o SEELE desenha» o cartão, o som do cartão do MOD seguiu, ou o da faixa parou junto: "
        + `${tocando()} fonte(s), cartão ${ditos(d, "cartao-7", "pausada").length}, faixa ${ditos(d, "faixa", "pausada").length}`,
    );
    // O MOD redeclara os cartões com `tocando: true`, e eles não voltam.
    regiao.declararCartoes(cartoes);
    await assentar();
    confere(caso, tocando() === 1, `o MOD redeclarou os cartões e um som calado voltou a tocar: ${tocando()} fonte(s)`);
    regiao.soltar();
  }

  // O destino que tomou os sons de uma contribuição e perde a disputa: o som
  // dela para lá, que é onde ele mora (T8 N1).
  {
    const Audio = audioDeMentira();
    const b = bancada({ audio: Audio });
    const d = dono(b, midiaDeSom);
    const sonsDaContribuicao = { tomados: false };
    const destinos = ["1", "2"].map(() => {
      const r = new b.RegiaoDeMod("a/b", d.api, b.raiz(), b.PERFIS.cartao);
      r.sonsDaContribuicao = sonsDaContribuicao;
      r.aplicar([{ forma: "midia", chave: "vinheta", fonte: "som/v.wav", tocando: true }]);
      return r;
    });
    await assentar();
    const tomou = destinos.find((r) => r.tomouOsSons);
    const outro = destinos.find((r) => r !== tomou);
    confere(caso, Boolean(tomou) && Audio.ultimo?.fontes[0]?.tocando === true, "a vinheta da contribuição não tocou");
    outro?.calarSonsForaDaTela(true);
    tomou?.calarSonsForaDaTela(false);
    confere(
      caso,
      Audio.ultimo?.fontes[0]?.parou === true && ditos(d, "vinheta", "pausada").length === 1,
      "o destino que segura o som da contribuição perdeu a disputa, e o som seguiu tocando: "
        + `parou=${Audio.ultimo?.fontes[0]?.parou}, «pausada» dita ${ditos(d, "vinheta", "pausada").length} vez(es)`,
    );
    for (const r of destinos) r.soltar();
  }
}

(async () => {
  const provas = [
    fundoTrocaSoltaECancela,
    oFocoSobreviveAAtualizacao,
    aPreviaNaoTrocaOsAncestraisDoCampo,
    oRodapeEReconciliadoSemMoverOBotao,
    oErroDoCampoAcompanhaAEdicaoSemRoubarFoco,
    oArrasteViraTracoAgregado,
    oArrastePegaAFiguraDeCima,
    sairDuranteOCarregamentoNaoMonta,
    aMidiaDoServidorTemOMesmoDono,
    sairDuranteAReproducaoPara,
    tirarUmNoSoltaOQueEleSegurava,
    osTetosContemEARecusaEDita,
    oQuadroPendenteNaoSobreviveASaida,
    tudoChegaAInstanciaESoltarDuasVezesNaoDoi,
    oCartaoUsaOMesmoRendererComGramaticaMenor,
    oCartaoSaiQuandoOModParaDeDeclararOuQuandoAReGiaoSai,
    oRetratoDoCartaoEOMesmoNoEntreDoisRetratos,
    osTetosDoCartaoSaoDoCartaoENaoDaRegiao,
    aImagemAcompanhaAMudancaDaFonte,
    cadaMidiaRecusadaEDitaAoAnfitriao,
    aMidiaAnotadaDizOsTresEstados,
    oFundoEORetratoTambemDizemOEstado,
    oSomTocaPorWebAudioENuncaPorUmElementoDeMidia,
    oSomDoServidorVemDosBytesDoDataENaoDeUmElemento,
    oSomQueNaoDecodificaEDito,
    oSomNumCartaoNaoTemBotao,
    oSomPedidoSemGestoEDitoRecusado,
    oAudioDaJanelaNaoSeguraASaidaAToa,
    aSuspensaoDoProdutoEsperaASaidaAcordar,
    osBytesDoSomChegamPelosDoisCaminhosDoIpc,
    asProtecoesDoTocadorEDaMontagemDoSom,
    oSomQueTerminouVoltaComOTocandoDeclarado,
    oSomQueSaiDaTelaParaComoOAudioParava,
  ];
  for (const prova of provas) {
    try {
      await prova();
    } catch (erro) {
      falhas.push(`${prova.name}: lançou ${erro?.stack ?? erro}`);
    }
  }
  terminou = true;
  if (falhas.length === 0) {
    console.log(`região do MOD: as ${provas.length} provas e a validação de resumo por linhas passam`);
    process.exit(0);
  }
  for (const falha of falhas) console.error(`FALHOU — ${falha}`);
  process.exit(1);
})();
