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
    this.className = "";
    this.style = {};
    this.ouvintes = new Map();
    this.value = "";
    this.paused = true;
    this.width = 0;
    this.height = 0;
    /** Foi tirado do documento alguma vez? É o que o foco mede. */
    this.saiuDoDocumento = 0;
  }
  get children() {
    return this.filhos.filter((f) => f.nodeType === ELEMENTO);
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
  // ---- o que um `<audio>` de mentira precisa ter ----
  pause() {
    this.paused = true;
  }
  play() {
    this.paused = false;
    return Promise.resolve();
  }
  load() {
    this.carregou = (this.carregou ?? 0) + 1;
  }
  removeAttribute(nome) {
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

/** Um contexto com o DOM mínimo e `mods-regiao.js` dentro. */
function bancada() {
  const quadros = [];
  const doc = { activeElement: null };
  doc.createElement = (tag) => new Elemento(tag, doc);
  doc.createTextNode = (data) => new NoDeTexto(data);

  const contexto = vm.createContext({
    console,
    document: doc,
    Node: { TEXT_NODE: TEXTO, ELEMENT_NODE: ELEMENTO },
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
    `${fonte}\nglobalThis.api = { RegiaoDeMod, LIMITES_DA_REGIAO, LIMITES_DO_CARTAO, FORMAS_DO_CARTAO };`,
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
    raiz: () => doc.createElement("section"),
  };
}

/** Um dono que anota o que a região fala e o que ela pede. */
function dono(b, midia) {
  const ditos = [];
  const registrados = [];
  return {
    ditos,
    registrados,
    api: {
      instancia: {
        registrar: (porque, descartar) => registrados.push({ porque, descartar }),
      },
      geracao: 1,
      podeFalar: () => true,
      falar: (dados) => ditos.push(dados),
      carregarMidia: (caminho) =>
        midia ? midia(caminho) : Promise.reject(new Error("sem mídia")),
      carregarMidiaDoServidor: (canal, pedido, campo) =>
        midia ? midia({ canal, pedido, campo }) : Promise.reject(new Error("sem mídia")),
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
  const terminou = d.ditos.at(-1);
  confere(
    caso,
    terminou?.fase === "terminou" && terminou?.alvo === "peca-de-cima" && terminou?.x === 150,
    `o fim do arraste não levou peça e destino: ${JSON.stringify(terminou)}`,
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
  const b = bancada();
  const d = dono(b, () =>
    Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 1024 }),
  );
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar([{ forma: "midia", chave: "m", fonte: "som/a.wav", tocando: true }]);
  await volta();
  await volta();

  const tocador = regiao.raiz.children[0].querySelector(".regiao-de-mod-tocador");
  confere(caso, !!tocador, "a mídia não foi montada");
  if (!tocador) return;
  confere(caso, tocador.paused === false, "o `tocando` declarado não tocou");
  confere(caso, regiao.bytesDeMidia === 1024, `os bytes não foram contados: ${regiao.bytesDeMidia}`);

  // A pessoa sai no meio do som.
  regiao.soltar();
  confere(caso, tocador.paused === true, "o som continuou tocando depois da saída");
  confere(
    caso,
    tocador.src === undefined,
    "a fonte não foi tirada, e os bytes decodificados ficam presos",
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
  const b = bancada();
  const d = dono(b, () =>
    Promise.resolve({ uri: "data:audio/wav;base64,AA", papel: "som", bytes: 512 }),
  );
  const regiao = new b.RegiaoDeMod("a/b", d.api, b.raiz());
  regiao.aplicar([
    { forma: "midia", chave: "m", fonte: "som/a.wav", tocando: true },
    { forma: "campo", chave: "c", rotulo: "X", valor: "" },
  ]);
  await volta();
  await volta();
  const tocador = regiao.raiz.children[0].querySelector(".regiao-de-mod-tocador");
  confere(caso, regiao.contagem.midias === 1 && regiao.contagem.campos === 1, "a conta inicial está errada");

  // O MOD redesenha **sem** a mídia: ela sai da tela, e o som tem de parar.
  regiao.aplicar([{ forma: "campo", chave: "c", rotulo: "X", valor: "" }]);
  confere(caso, tocador.paused === true, "o som continuou depois de o MOD tirar a mídia da tela");
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

(async () => {
  const provas = [
    oFocoSobreviveAAtualizacao,
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
  ];
  for (const prova of provas) {
    try {
      await prova();
    } catch (erro) {
      falhas.push(`${prova.name}: lançou ${erro?.stack ?? erro}`);
    }
  }
  if (falhas.length === 0) {
    console.log(`região do MOD: as ${provas.length} provas passam`);
    process.exit(0);
  }
  for (const falha of falhas) console.error(`FALHOU — ${falha}`);
  process.exit(1);
})();
