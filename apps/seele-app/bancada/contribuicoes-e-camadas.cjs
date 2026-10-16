// As correções da revisão de 20/09/2026, medidas no HTML e no roteador reais.
//
// # Por que esta bancada existe
//
// A revisão encontrou seis defeitos com reproduções pontuais e escreveu, na
// primeira linha do script dela: «asserções descrevem os defeitos encontrados;
// devem ser invertidas ao corrigi-los». Este arquivo é a inversão, e ele fica:
// uma reprodução que some depois do conserto não guarda nada.
//
// # O que separa esta bancada do laboratório
//
// **A página.** O defeito R1 não era `inertarFora` estar errado em abstrato:
// era o palco das camadas estar **dentro** de `section#tela-sessao` no
// `index.html`, e o preview dos MODs pô-lo direto no `<body>`. O mesmo código
// passava num e falhava no outro. Por isso a árvore aqui não é escrita à mão:
// ela é lida de `ui/index.html`, com os mesmos pais e os mesmos irmãos que a
// aplicação tem. Um dia em que alguém mover `#palco-de-camadas` de lugar, esta
// bancada mede o lugar novo.
//
// **O roteador.** R2 tem duas metades — quem pode revogar o quê, e que
// mensagens uma API antiga alcança —, e as duas moram em `atenderOMod`. Medir
// o registro sozinho não veria nenhuma das duas: o registro nem sabe quem
// mandou a mensagem.
//
// # O que ela não prova
//
// Não é navegador: `inert` aqui é uma propriedade que ninguém propaga, e o
// foco não existe. O que se mede é **a quem** o produto atribui inércia, que é
// exatamente o que estava errado. O resto — que o Tab pare na borda, que a
// confirmação apareça por cima — é observação nativa, e continua sendo.

const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const raiz = path.resolve(__dirname, "..");
const ler = (nome) => fs.readFileSync(path.join(raiz, "ui", nome), "utf8");

const falhas = [];
function confere(caso, condicao, detalhe) {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
}

// **O fim é dito, e não suposto.** Uma promessa que nunca se resolve esvazia o
// laço de eventos sem erro nenhum: o node sai com 0, sem uma linha, e o passo
// do CI fica verde sem ter provado nada — medido com `await new Promise(() => {})`
// em `atenderOMod`. O `terminou` só vira verdade em `terminar()`.
//
// Só quando o código seria 0: uma exceção solta também passa por aqui, mas já
// sai com 1 e com a pilha dela, e dizer «promessa pendurada» por cima seria
// apontar a causa errada.
let terminou = false;
process.on("exit", (codigo) => {
  if (!terminou && codigo === 0) {
    console.error("contribuicoes-e-camadas: não chegou ao fim — uma promessa ficou pendurada");
    process.exitCode = 1;
  }
});

// ------------------------------------------------- a árvore que a página tem

/** Etiquetas que não fecham, e por isso não empilham. */
const VAZIAS = new Set([
  "area", "base", "br", "col", "embed", "hr", "img", "input",
  "link", "meta", "source", "track", "wbr",
]);

class No {
  constructor(tag, id) {
    this.tagName = String(tag).toUpperCase();
    this.id = id || "";
    this.parentNode = null;
    this.filhos = [];
    this.inert = false;
    this.hidden = false;
    this.dataset = {};
    this.ouvintes = 0;
  }
  get children() {
    return this.filhos;
  }
  get isConnected() {
    let no = this;
    while (no.parentNode) no = no.parentNode;
    return no.raizDoDocumento === true;
  }
  append(...nos) {
    for (const no of nos) {
      no.parentNode?.remover(no);
      no.parentNode = this;
      this.filhos.push(no);
    }
  }
  remover(no) {
    const onde = this.filhos.indexOf(no);
    if (onde >= 0) this.filhos.splice(onde, 1);
  }
  remove() {
    this.parentNode?.remover(this);
    this.parentNode = null;
  }
  addEventListener() { this.ouvintes += 1; }
  removeEventListener() { this.ouvintes -= 1; }
  focus() { documento.activeElement = this; }
}

/**
 * Monta a árvore do `index.html` por pilha de etiquetas.
 *
 * Só etiquetas, `id` e aninhamento: é o que `inertarFora` percorre. Texto,
 * atributos e o conteúdo dos `<script>` não entram — e o conteúdo dos scripts
 * é pulado de propósito, porque um `<` dentro de JavaScript não é uma etiqueta.
 */
function lerAPagina() {
  let html = ler("index.html");
  html = html.replace(/<!--[\s\S]*?-->/g, "");
  html = html.replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, "");
  html = html.replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, "");

  const documentoRaiz = new No("#document");
  documentoRaiz.raizDoDocumento = true;
  const porId = new Map();
  const pilha = [documentoRaiz];
  const etiqueta = /<(\/?)([a-zA-Z][\w-]*)([^>]*?)(\/?)>/g;

  for (const achado of html.matchAll(etiqueta)) {
    const [, fecha, tag, atributos, sozinha] = achado;
    const nome = tag.toLowerCase();
    if (fecha) {
      for (let i = pilha.length - 1; i > 0; i -= 1) {
        if (pilha[i].tagName === nome.toUpperCase()) {
          pilha.length = i;
          break;
        }
      }
      continue;
    }
    const id = /\bid\s*=\s*"([^"]*)"/.exec(atributos)?.[1] ?? "";
    const no = new No(nome, id);
    pilha[pilha.length - 1].append(no);
    if (id) porId.set(id, no);
    if (!VAZIAS.has(nome) && !sozinha) pilha.push(no);
  }
  return { documentoRaiz, porId };
}

const { documentoRaiz, porId } = lerAPagina();

/** O primeiro nó com esta etiqueta, em profundidade. */
function procurar(no, tag) {
  if (no.tagName === tag) return no;
  for (const filho of no.filhos) {
    const achado = procurar(filho, tag);
    if (achado) return achado;
  }
  return null;
}

const teclados = new Set();
const documento = {
  addEventListener: (nome, fn) => { if (nome === "keydown") teclados.add(fn); },
  removeEventListener: (nome, fn) => { if (nome === "keydown") teclados.delete(fn); },
  activeElement: null,
  body: procurar(documentoRaiz, "BODY"),
  getElementById: (id) => porId.get(id) ?? null,
};

confere(
  "a página",
  documento.body && documento.body.tagName === "BODY",
  "o `<body>` do index.html não foi encontrado pela leitura da árvore",
);
const camadas = porId.get("palco-de-camadas");
const sessao = porId.get("tela-sessao");
const moderar = porId.get("moderar");
confere("a página", Boolean(camadas), "`#palco-de-camadas` sumiu do index.html");
confere("a página", Boolean(sessao), "`#tela-sessao` sumiu do index.html");
confere("a página", Boolean(moderar), "`#moderar` sumiu do index.html");
confere("confirmação sem sessão", moderar?.parentNode === documento.body,
  "a confirmação está sob uma tela que pode estar escondida nas configurações");

/**
 * **A estrutura que fez R1 passar despercebido.**
 *
 * Se um dia o palco virar filho do `<body>`, esta bancada passa a medir a
 * mesma coisa que o laboratório media — e aí ela deixa de provar o que existe
 * para provar. Melhor reprovar aqui e mudar o caso de propósito.
 */
confere(
  "R1 · a estrutura",
  camadas && camadas.parentNode === sessao,
  "`#palco-de-camadas` não está mais dentro de `#tela-sessao`: o caso que a "
  + "revisão encontrou mudou de forma, e esta bancada precisa acompanhar",
);

// ------------------------------------------ os arquivos do produto, de verdade

const contexto = vm.createContext({
  console,
  queueMicrotask: () => {},
  setTimeout: () => 1,
  clearTimeout: () => {},
  requestAnimationFrame: (f) => f(),
  document: documento,
  geracaoDaSessao: 7,
  modsCarregados: new Map(),
  registrarNoAnfitriao() {},
});
vm.runInContext(
  `${ler("mods-runtime.js")}\n${ler("mods-contribuicoes.js")}\n${ler("mods-superficies.js")}\n`
  + "this.R = RegistroDeContribuicoes; this.S = SuperficieDeMod; this.SS = SuperficiesDoMod;"
  + "this.I = InstanciaDeMod; this.PONTOS = PONTOS_DE_CONTRIBUICAO;"
  + "this.NATIVO = NATIVO; this.ESTADOS_DE_MOD = ESTADOS_DE_MOD;",
  contexto,
);
const { R, S, I, PONTOS, NATIVO } = contexto;
// **As frases de tela, inteiras, como a página as carrega.** O `catch` de
// `atenderOMod` responde ao MOD por `fraseDeErro`, e sem este arquivo ele
// tomava o outro ramo do `typeof`: a bancada media um caminho que o app não
// roda, e foi assim que o registro pôs «ALGO FALHOU E ESTE APP NÃO SABE
// EXPLICAR O QUÊ» na frente do motivo sem que nada aqui reprovasse. Num
// `runInContext` só dele porque o arquivo abre com `"use strict"`, que só vale
// no começo de um script. Medido em 05/10/2026: toda chamada que `fraseDeErro`
// recebe nesta bancada vem de `atenderOMod`, no R2, e nenhum outro bloco muda
// de resultado com o arquivo no contexto.
vm.runInContext(ler("frases.js"), contexto);

// ------------------------------------------------------- R1 · o ramo do modal

/** Um diálogo de mentira com uma camada de verdade no palco das camadas. */
function dialogoDeTeste() {
  const camada = new No("div", "");
  camadas.append(camada);
  return {
    palcos: { camadas },
    camada,
    raiz: new No("div", ""),
    focarPrimeiro() {},
    // O furo do véu (tarefa 9, Step 4b) mede retângulos, que este DOM não tem;
    // a bancada `nucleo.cjs` o mede no navegador.
    vigiarORecorte() {},
    // O dono, que `fechar` e `descartar` avisam (a prévia do tema, 17g).
    dono: {},
    solta: false,
  };
}

/** Apaga toda inércia da árvore, para um caso começar do zero. */
function acordarTudo(no) {
  no.inert = false;
  for (const filho of no.filhos) acordarTudo(filho);
}

{
  const dialogo = dialogoDeTeste();
  S.prototype.prenderFoco.call(dialogo);

  // **O ancestral do diálogo continua acordado.** A reprodução da revisão
  // afirmava o contrário — `assert.equal(sessao.inert, true)` —, e era isso
  // que tornava o próprio modal inalcançável: `inert` desce.
  confere("R1 · o ramo ativo", sessao.inert !== true, "`#tela-sessao` foi marcada inerte, e o diálogo está dentro dela");
  confere("R1 · o ramo ativo", camadas.inert !== true, "o próprio palco das camadas foi marcado inerte");
  confere("R1 · o ramo ativo", dialogo.camada.inert !== true, "a camada do diálogo foi marcada inerte");
  let acima = camadas.parentNode;
  while (acima && acima !== documentoRaiz) {
    confere("R1 · o ramo ativo", acima.inert !== true, `o ancestral <${acima.tagName.toLowerCase()}${acima.id ? `#${acima.id}` : ""}> ficou inerte`);
    acima = acima.parentNode;
  }

  // **E os irmãos, em todos os níveis, dormem.** Um modal que não adormece o
  // resto é a armadilha clássica: parece modal e responde a Tab.
  const irmaosNaSessao = sessao.children.filter((n) => n !== camadas);
  confere(
    "R1 · o resto dorme",
    irmaosNaSessao.length > 0 && irmaosNaSessao.every((n) => n.inert === true),
    "irmãos de `#palco-de-camadas` dentro de `#tela-sessao` continuaram alcançáveis: "
    + irmaosNaSessao.filter((n) => n.inert !== true).map((n) => n.id || n.tagName).join(", "),
  );
  const irmaosNoCorpo = documento.body.children.filter((n) => n !== sessao);
  confere(
    "R1 · o resto dorme",
    irmaosNoCorpo.length > 0 && irmaosNoCorpo.every((n) => n.inert === true),
    "irmãos de `#tela-sessao` no `<body>` continuaram alcançáveis: "
    + irmaosNoCorpo.filter((n) => n.inert !== true).map((n) => n.id || n.tagName).join(", "),
  );
  confere("R1 · o resto dorme", moderar.inert === true, "`#moderar` ficou acordado com um modal de MOD aberto");

  S.prototype.soltarFoco.call(dialogo);
  confere(
    "R1 · o despertar",
    irmaosNaSessao.every((n) => n.inert === false) && irmaosNoCorpo.every((n) => n.inert === false),
    "alguém continuou inerte depois de soltar o foco",
  );
  dialogo.camada.remove();
  acordarTudo(documento.body);
}

// ------------------------------- R1c · reabrir a mesma superfície é idempotente

{
  // **O caminho público:** `criar` com o mesmo `id` reaproveita a superfície e
  // chama `mostrar`, que desde o conserto de R6 chama `abrir`, que chama
  // `prenderFoco`. Uma segunda passagem por ali não pode deixar rastro.
  //
  // Antes: a segunda chamada não adormecia ninguém — já estava tudo inerte — e
  // **substituía** a lista de adormecidos pela lista vazia. Fechar depois disso
  // devolvia nada, e a aplicação ficava inerte sem nada na tela para explicar.
  const dialogo = dialogoDeTeste();
  const fundo = sessao.children.find((n) => n !== camadas);

  S.prototype.prenderFoco.call(dialogo);
  confere("R1c · reabrir", fundo.inert === true, "a primeira abertura não adormeceu o fundo");
  S.prototype.prenderFoco.call(dialogo);
  S.prototype.prenderFoco.call(dialogo);
  confere("R1c · reabrir", fundo.inert === true, "reabrir acordou o fundo com o modal ainda aberto");
  confere(
    "R1c · reabrir",
    teclados.size === 1,
    `reabrir registrou ${teclados.size} ouvintes de teclado; um é o certo`,
  );

  S.prototype.soltarFoco.call(dialogo);
  confere(
    "R1c · reabrir",
    fundo.inert === false,
    "fechar depois de reabrir deixou o fundo inerte: a aplicação fica travada sem nada na tela para explicar",
  );
  confere("R1c · reabrir", teclados.size === 0, "sobrou ouvinte de teclado depois de fechar");
  dialogo.camada.remove();
  acordarTudo(documento.body);
}

// Escape chega ao documento quando um clique no WebKit não foca o botão.
{
  const a = dialogoDeTeste();
  const b = dialogoDeTeste();
  const fechados = [];
  a.pedirFechamento = () => fechados.push("a");
  b.pedirFechamento = () => fechados.push("b");
  S.prototype.prenderFoco.call(a);
  S.prototype.prenderFoco.call(b);
  documento.activeElement = documento.body;
  const escape = () => {
    const evento = { key: "Escape", defaultPrevented: false,
      preventDefault() { this.defaultPrevented = true; } };
    for (const ouvir of teclados) ouvir(evento);
  };
  escape();
  confere("Escape sem foco", fechados.join() === "b", "Escape não chegou somente ao modal do topo");
  S.prototype.soltarFoco.call(a);
  S.prototype.soltarFoco.call(b);
  a.camada.remove();
  b.camada.remove();
  confere("Escape sem foco", teclados.size === 0, "a saída reteve o teclado global");
  acordarTudo(documento.body);
}

// ------------------------------- R1d · dois modais, fechados fora de ordem

{
  const fundo = sessao.children.find((n) => n !== camadas);
  const a = dialogoDeTeste();
  const b = dialogoDeTeste();

  S.prototype.prenderFoco.call(a);
  S.prototype.prenderFoco.call(b);
  confere("R1d · dois modais", fundo.inert === true, "com dois modais abertos o fundo estava acordado");
  // B é o topo: a camada de A fica atrás dela, e atrás quer dizer inerte.
  confere("R1d · dois modais", a.camada.inert === true, "a camada de baixo continuou alcançável por trás da de cima");
  confere("R1d · dois modais", b.camada.inert !== true, "a camada do topo foi marcada inerte");

  // **A fecha primeiro.** É o caso normal, não o exótico: o MOD de A respondeu
  // ao `fechar-pedido` enquanto a confirmação de B estava de pé.
  S.prototype.soltarFoco.call(a);
  confere(
    "R1d · fora de ordem",
    fundo.inert === true,
    "fechar o modal de baixo liberou o fundo com o de cima ainda aberto",
  );
  confere("R1d · fora de ordem", b.camada.inert !== true, "o modal que continua aberto ficou inerte");

  S.prototype.soltarFoco.call(b);
  confere("R1d · fora de ordem", fundo.inert === false, "fechar os dois não devolveu o fundo");
  confere("R1d · fora de ordem", a.camada.inert === false, "a camada de baixo ficou inerte depois de tudo fechado");
  a.camada.remove();
  b.camada.remove();
  acordarTudo(documento.body);
}

// ------------------------------- R1b · a confirmação do produto fica alcançável

{
  const falas = [];
  const dialogo = dialogoDeTeste();
  Object.assign(dialogo, {
    id: "mod/x",
    titulo: "Perfil",
    chave: "mod/x:perfil",
    dono: { falar: (e) => falas.push(e) },
    fechar() { this.fechou = true; },
    fechou: false,
  });
  S.prototype.prenderFoco.call(dialogo);
  confere("R1b", moderar.inert === true, "`#moderar` precisava estar dormindo antes da pergunta");

  let pedido = null;
  contexto.abrirConfirmacao = (titulo, consequencia, rotulo, executar, aoFechar) => {
    pedido = { titulo, consequencia, rotulo, executar, aoFechar };
  };
  S.prototype.confirmarDescarte.call(dialogo, "escape");

  confere("R1b", pedido !== null, "a confirmação de descarte não foi aberta");
  // **Acordada enquanto a pergunta está de pé.** Uma pergunta inerte é uma
  // janela que não fecha: o diálogo do MOD não sai, e a caixa não responde.
  confere("R1b", moderar.inert === false, "a confirmação do produto ficou inerte atrás do modal do MOD");

  // **E outra camada subindo no meio não adormece a pergunta.** Um MOD pode
  // criar uma superfície enquanto alguém lê a confirmação; o recálculo devolve
  // tudo o que é nosso, e sem o pedido explícito a pergunta voltaria a dormir
  // com ela na tela.
  const intrusa = dialogoDeTeste();
  S.prototype.prenderFoco.call(intrusa);
  confere("R1b", moderar.inert === false, "uma camada nova adormeceu a confirmação que estava aberta");
  let fechouPorTras = false;
  intrusa.pedirFechamento = () => { fechouPorTras = true; };
  const tecla = { key: "Escape", preventDefault() {} };
  for (const ouvir of teclados) ouvir(tecla);
  confere("Escape na confirmação", !fechouPorTras, "Escape fechou um MOD atrás da confirmação do produto");
  S.prototype.soltarFoco.call(intrusa);
  intrusa.camada.remove();
  confere("R1b", moderar.inert === false, "a camada intrusa saindo adormeceu a confirmação que estava aberta");

  // Cancelar devolve o estado anterior: a pergunta saiu, o modal do MOD
  // continua de pé, e a moderação volta a dormir.
  pedido.aoFechar();
  confere("R1b", moderar.inert === true, "cancelar a confirmação deixou `#moderar` acordado por baixo do modal");

  // Confirmar fecha o diálogo do MOD.
  pedido.executar();
  confere("R1b", dialogo.fechou === true, "confirmar o descarte não fechou a superfície");
  confere(
    "R1b",
    falas.some((f) => f.nome === "fechar" && f.descartou === true),
    "o MOD não foi avisado de que a janela dele fechou descartando",
  );
  S.prototype.soltarFoco.call(dialogo);
  dialogo.camada.remove();
  acordarTudo(documento.body);
  delete contexto.abrirConfirmacao;
}

// ------------------------------------------- R6 · visibilidade e montagem

{
  const palco = new No("div", "palco-teste");
  documento.body.append(palco);
  const camadaDoTeste = new No("div", "");
  const superficie = Object.create(S.prototype);
  Object.assign(superficie, {
    tipo: "dialogo",
    modal: false,
    chave: "mod/x:janela",
    solta: false,
    visivel: true,
    inertes: [],
    palcos: { camadas: palco },
    camada: camadaDoTeste,
    raiz: new No("div", ""),
    renderer: {
      soltar() { superficie.soltou = true; },
      // Anota se a página ainda estava no documento quando pediu: o som que se
      // cala é o que saiu dele, e pedir antes de tirar o nó não cala nada.
      calarSonsForaDaTela() { superficie.calouNoDocumento.push(superficie.montada); },
    },
    soltou: false,
    calouNoDocumento: [],
    // O dono, que `fechar` e `descartar` avisam (a prévia do tema, 17g).
    dono: {},
  });

  superficie.abrir();
  confere("R6 · criar", superficie.montada === true, "abrir não pôs a superfície no palco");

  superficie.ocultar();
  confere("R6 · ocultar", superficie.visivel === false, "ocultar não marcou invisível");
  confere("R6 · ocultar", superficie.montada === true, "ocultar tirou o nó do documento: isso é fechar");
  confere("R6 · ocultar", camadaDoTeste.hidden === true, "ocultar não escondeu a camada");

  superficie.mostrar();
  confere("R6 · mostrar", superficie.visivel === true, "mostrar não marcou visível");
  confere("R6 · mostrar", camadaDoTeste.hidden === false, "mostrar não revelou a camada");

  superficie.fechar();
  confere("R6 · fechar", superficie.montada === false, "fechar deixou o nó no palco");
  // **E o som dela para.** Fechar tira o nó do documento e mantém o renderer
  // para reabrir; uma fonte de WebAudio não para por sair do documento, como o
  // `<audio>` parava — quem fecha a MESA continuava ouvindo a trilha (I-1 da
  // revisão ampla do Plano 1D). O som da região é medido em `regiao-do-mod.cjs`.
  confere(
    "R6 · fechar",
    JSON.stringify(superficie.calouNoDocumento) === JSON.stringify([false]),
    "fechar não pediu ao renderer que calasse o som da página depois de tirá-la do documento "
      + `(${JSON.stringify(superficie.calouNoDocumento)}): um som de MOD continua tocando com a página fechada`,
  );

  // **A correção de R6.** Antes, isto devolvia sucesso e a janela não voltava.
  superficie.mostrar();
  confere(
    "R6 · mostrar depois de fechar",
    superficie.montada === true,
    "`mostrar` disse que a superfície está visível com ela fora do documento — "
    + "o contrato que a revisão encontrou quebrado",
  );
  confere("R6 · mostrar depois de fechar", superficie.visivel === true, "mostrar não remarcou visível");

  // Idempotência: mostrar duas vezes não duplica nem move nada.
  superficie.mostrar();
  confere(
    "R6 · idempotência",
    palco.children.filter((n) => n === camadaDoTeste).length === 1,
    "mostrar duas vezes pôs a camada no palco duas vezes",
  );

  superficie.descartar();
  confere("R6 · descartar", superficie.montada === false, "descartar deixou o nó no palco");
  confere("R6 · descartar", superficie.soltou === true, "descartar não soltou o renderer");
  superficie.descartar();
  confere("R6 · descartar", superficie.solta === true, "descartar duas vezes desfez o estado");

  // **Uma superfície descartada não devolve sucesso.**
  let recusou = false;
  try { superficie.mostrar(); } catch { recusou = true; }
  confere("R6 · descartada", recusou, "`mostrar` numa superfície descartada devolveu sucesso");
  palco.remove();
}

// -------------------------------------- R5 · automático, provedor e nativo

{
  const registro = new R();
  const a = new I("mod/a", "a", 7, {});
  const b = new I("mod/b", "b", 7, {});
  registro.registrar({ id: "mod/a" }, a, { ponto: "pessoa.cartao", modo: "substituir", prioridade: 10 });
  registro.registrar({ id: "mod/b" }, b, { ponto: "pessoa.cartao", modo: "substituir", prioridade: 5 });

  const automatico = registro.escolherSubstituicao("pessoa.cartao", "", "");
  confere("R5 · automático", automatico.escolhida?.mod === "mod/a", "sem preferência, a prioridade deixou de decidir");
  confere("R5 · automático", automatico.nativa === false, "o automático foi confundido com o nativo");
  confere("R5 · automático", automatico.preteridas.length === 1, "a disputa deixou de ser visível na gestão");

  const escolhido = registro.escolherSubstituicao("pessoa.cartao", "", "mod/b");
  confere("R5 · provedor", escolhido.escolhida?.mod === "mod/b", "a escolha do servidor não foi respeitada");
  confere("R5 · provedor", escolhido.nativa === false, "escolher um provedor foi lido como nativo");

  // **O terceiro estado, que era o defeito.** «Usar apresentação padrão»
  // devolvia à seleção automática de MODs — ou seja, ao `mod/a` acima.
  const nativo = registro.escolherSubstituicao("pessoa.cartao", "", NATIVO);
  confere("R5 · nativo", nativo.escolhida === null, `o nativo explícito ainda escolheu «${nativo.escolhida?.mod}»`);
  confere("R5 · nativo", nativo.nativa === true, "o nativo explícito não foi relatado como nativo");
  // **E ele não é «o provedor escolhido sumiu».** Os dois desfechos desenham a
  // mesma coisa — o cartão nativo —, e a gestão diz coisas diferentes sobre
  // eles: um é uma escolha, o outro é um aviso. Sem esta linha, apagar o ramo
  // do nativo explícito faz o valor reservado cair no caminho do provedor
  // ausente, e a bancada não vê diferença.
  confere(
    "R5 · nativo",
    nativo.ausente === undefined,
    "o nativo explícito foi relatado como um provedor que sumiu: "
    + `«${nativo.ausente}» não é um MOD, é a escolha de não ter nenhum`,
  );

  // Um provedor escolhido que saiu não vira silêncio nem vira outro MOD.
  const ausente = registro.escolherSubstituicao("pessoa.cartao", "", "mod/z");
  confere("R5 · ausente", ausente.ausente === "mod/z", "o provedor escolhido que sumiu não foi nomeado");

  // **E a chave da preferência tem destino.** O registro de decisões dizia
  // «por destino», e a chave era global por ponto.
  const camada = ler("camada-mods.js");
  confere(
    "R5 · a chave",
    /function chaveDaPreferencia\(/.test(camada),
    "a preferência voltou a ser chaveada só pelo ponto, sem destino",
  );
}

// ------------------------------------------- M1 · quem pinta cada lugar

{
  // **Só leitura, e por alvo.** `resumo` respondia só pelo alvo vazio, e por
  // isso não via uma substituição registrada para uma pessoa só — que é como
  // todo avatar do PERFIS é registrado. `quemPinta` pergunta a mesma
  // `escolherSubstituicao` para cada alvo que tem contribuição, e `resumo`
  // passou a perguntar a ele no caso parcial e no automático misto.
  const registro = new R();
  const a = new I("mod/a", "a", 7, {});
  const b = new I("mod/b", "b", 7, {});
  const c = new I("mod/c", "c", 7, {});
  registro.registrar({ id: "mod/a" }, a, { ponto: "pessoa.cartao", modo: "substituir", prioridade: 10 });
  registro.registrar({ id: "mod/b" }, b, { ponto: "pessoa.cartao", modo: "substituir", prioridade: 5 });
  registro.registrar({ id: "mod/c" }, c, { ponto: "pessoa.cartao", modo: "adicionar" });

  const auto = registro.quemPinta("pessoa.cartao", "");
  confere("M1 · quem pinta", auto.substituivel === true, "`pessoa.cartao` aceita substituir e não foi lido como disputável");
  confere("M1 · quem pinta", auto.substitui.join() === "mod/a", `sem preferência, quem vale deixou de ser o de maior prioridade: ${auto.substitui}`);
  confere("M1 · quem pinta", auto.perderam.join() === "mod/b", `quem perdeu a disputa não foi dito: ${auto.perderam}`);
  confere("M1 · quem pinta", auto.acrescentam.join() === "mod/c", `quem só acrescenta sumiu da resposta: ${auto.acrescentam}`);
  confere("M1 · quem pinta", auto.contribuicoes === 3, `contou ${auto.contribuicoes} contribuições, e são três`);

  const escolhido = registro.quemPinta("pessoa.cartao", "mod/b");
  confere(
    "M1 · preferência",
    escolhido.substitui.join() === "mod/b" && escolhido.perderam.join() === "mod/a",
    `a escolha desta máquina não decidiu: vale ${escolhido.substitui}, perdeu ${escolhido.perderam}`,
  );

  const nativo = registro.quemPinta("pessoa.cartao", NATIVO);
  confere("M1 · nativo", nativo.nativa === true && nativo.substitui.length === 0, "com o SEELE escolhido, algum MOD continuou valendo");
  confere("M1 · nativo", [...nativo.perderam].sort().join() === "mod/a,mod/b", `com o SEELE escolhido, os dois perdem: ${nativo.perderam}`);
  confere("M1 · nativo", nativo.ausente === "", `o valor reservado foi lido como um MOD que sumiu: «${nativo.ausente}»`);

  const sumiu = registro.quemPinta("pessoa.cartao", "mod/z");
  confere("M1 · ausente", sumiu.ausente === "mod/z" && sumiu.nativa === true, "o provedor escolhido que não está de pé não foi nomeado");

  // **Por alvo.** Uma substituição registrada só para a pessoa 12 vale para ela.
  registro.registrar({ id: "mod/b" }, b, { ponto: "pessoa.cartao", modo: "substituir", alvo: "12", prioridade: 50 });
  const porAlvo = registro.quemPinta("pessoa.cartao", "");
  confere("M1 · por alvo", [...porAlvo.substitui].sort().join() === "mod/a,mod/b", `a substituição de uma pessoa só não apareceu: ${porAlvo.substitui}`);
  confere("M1 · por alvo", porAlvo.perderam.length === 0, `quem vale para uma pessoa foi contado também como quem perdeu: ${porAlvo.perderam}`);

  // **Uma escolha que vale numa pessoa e não noutra.** O avatar é sempre por
  // pessoa: `mod/a` desenha a 12, `mod/b` desenha a 7, e esta máquina escolheu
  // `mod/b`. Na 7 vale `mod/b`; na 12 o escolhido não tem candidata e o SEELE
  // desenha. `nativa` e `ausente` falam do ponto inteiro, e o ponto tem quem o
  // desenhe: dizer que `mod/b` «não está de pé» seria falso e esconderia que
  // ele desenha a pessoa 7. O caso parcial é dito por `substitui` e `perderam`.
  const avatares = new R();
  const avatar = { ponto: "pessoa.avatar", modo: "substituir", conteudo: { doServidor: { canal: 1, pedido: {} } } };
  avatares.registrar({ id: "mod/a" }, a, { ...avatar, alvo: "12" });
  avatares.registrar({ id: "mod/b" }, b, { ...avatar, alvo: "7" });
  const parcial = avatares.quemPinta("pessoa.avatar", "mod/b");
  confere(
    "M1 · por alvo",
    parcial.substitui.join() === "mod/b" && parcial.perderam.join() === "mod/a",
    `num avatar por pessoa, a escolha desta máquina não decidiu: vale «${parcial.substitui}», perdeu «${parcial.perderam}»`,
  );
  // **E a pessoa 12 é dita.** O escolhido não tem candidata nela, e o SEELE a
  // desenha: sem este campo, a gestão diria «desenhado por mod/b» de um ponto
  // que o SEELE desenha em parte.
  confere(
    "M1 · por alvo",
    parcial.oSeeleDesenhaOutros === true,
    "num avatar por pessoa, a pessoa 12 (que só mod/a declarou) fica com o SEELE pela escolha desta máquina, "
    + `e quem pinta não disse: ${JSON.stringify(parcial)}`,
  );
  confere(
    "M1 · por alvo",
    parcial.ausente === "",
    `o MOD escolhido desenha a pessoa 7 e foi dito como quem não está de pé: «${parcial.ausente}»`,
  );
  confere(
    "M1 · por alvo",
    parcial.nativa === false,
    "o MOD escolhido desenha a pessoa 7, e o ponto foi dito como desenhado pelo SEELE por escolha desta máquina",
  );
  // E o escolhido que não está de pé em pessoa nenhuma continua dito, por alvo.
  const emNenhuma = avatares.quemPinta("pessoa.avatar", "mod/z");
  confere(
    "M1 · por alvo",
    emNenhuma.ausente === "mod/z" && emNenhuma.nativa === true && emNenhuma.substitui.length === 0,
    `o provedor escolhido não está de pé em pessoa nenhuma e não foi nomeado: ${JSON.stringify(emNenhuma)}`,
  );
  // O campo é do caso **parcial**: quando o SEELE desenha o ponto inteiro, quem
  // diz é `nativa`; quando todo alvo disputado tem vencedora, ninguém sobra.
  const semSobra = {
    "o automático": auto,
    "a escolha de mod/b no cartão": escolhido,
    "o SEELE escolhido": nativo,
    "um escolhido que não está de pé": sumiu,
    "o automático por alvo": porAlvo,
    "um avatar escolhido que não está de pé em pessoa nenhuma": emNenhuma,
  };
  confere(
    "M1 · por alvo",
    Object.values(semSobra).every((resposta) => resposta.oSeeleDesenhaOutros === false),
    "«o SEELE desenha os outros» foi dito sem caso parcial — o SEELE desenha o ponto inteiro, ou todo alvo "
    + "disputado tem vencedora: "
    + Object.entries(semSobra)
      .filter(([, resposta]) => resposta.oSeeleDesenhaOutros !== false)
      .map(([caso, resposta]) => `${caso} (${resposta.oSeeleDesenhaOutros})`)
      .join(", "),
  );

  // Um ponto sem ninguém responde vazio, e não some; um que não existe, nada.
  const vazio = registro.quemPinta("compositor.ferramentas", "");
  confere(
    "M1 · vazio",
    vazio.contribuicoes === 0 && vazio.substituivel === false && vazio.acrescentam.length === 0,
    `um ponto sem contribuição respondeu ${JSON.stringify(vazio)}`,
  );
  confere("M1 · vazio", registro.quemPinta("nao.existe", "") === null, "um ponto que a API não conhece ganhou resposta");

  // **E nada mudou.** Ler a disputa não a decide de novo, nem mexe no registro.
  confere("M1 · só leitura", registro.porHandle.size === 4, `o registro tem ${registro.porHandle.size} contribuições depois das leituras, e eram quatro`);
  // Ler um ponto vazio não o cria: `tirar` não deixa entrada vazia em
  // `porPonto`, e uma que a leitura deixasse viraria, no `resumo`, uma linha
  // com zero contribuições.
  confere(
    "M1 · só leitura",
    registro.porPonto.size === 1,
    `o registro tem ${registro.porPonto.size} pontos depois das leituras, e só «pessoa.cartao» tem contribuição: `
    + `ler um ponto vazio o criou (${[...registro.porPonto.keys()].join(", ")})`,
  );
  confere(
    "M1 · só leitura",
    registro.escolherSubstituicao("pessoa.cartao", "", "").escolhida?.mod === "mod/a",
    "ler quem pinta mudou quem a disputa escolhe",
  );
}

// ------------------------------------------ R4 · registrar e revogar estabiliza

{
  const registro = new R();
  const dono = new I("mod/a", "a", 7, {});
  for (let n = 0; n < 1000; n += 1) {
    const { handle } = registro.registrar({ id: "mod/a" }, dono, {
      ponto: "canal.item", alvo: "1", conteudo: { texto: `conteúdo ${n}` },
    });
    registro.revogar(handle, { id: "mod/a", instancia: dono });
  }
  confere("R4", registro.porHandle.size === 0, `sobraram ${registro.porHandle.size} contribuições vivas`);
  // A reprodução da revisão terminava aqui com 1000.
  confere("R4", dono.recursos.length === 0, `sobraram ${dono.recursos.length} descartadores retidos na instância`);
  confere("R4", (registro.porMod.get("mod/a") ?? 0) === 0, "a cota do MOD não voltou a zero");

  // E revogar o mesmo punho duas vezes não desconta duas vezes.
  const { handle } = registro.registrar({ id: "mod/a" }, dono, { ponto: "canal.item", alvo: "1" });
  registro.revogar(handle, { id: "mod/a", instancia: dono });
  const deNovo = registro.revogar(handle, { id: "mod/a", instancia: dono });
  confere("R4", deNovo === false, "revogar duas vezes disse que havia o que tirar");
  confere("R4", dono.recursos.length === 0, "revogar duas vezes deixou descartador para trás");
}

// ------------------------- R4b · o descarte de quem **montou** alguma coisa

/**
 * O renderer de mentira, e por que ele basta aqui.
 *
 * O que se mede nesta seção é **registro de recursos**: quem sai, quando, e o
 * que sobra na instância. O desenho de verdade é medido em `regiao-do-mod.cjs`
 * e no laboratório dos três MODs, num navegador.
 *
 * O que ele conta é quantas vezes `soltar` foi chamado — que é a pergunta que
 * a revisão de 26ad0c2 fez e que a medida anterior não respondia, porque mil
 * ciclos de uma contribuição **sem conteúdo** nunca passam por aqui.
 */
let soltouRenderer = 0;
/** Os renderers que a montagem criou, na ordem: o que cada um recebeu é o que se confere. */
const renderersMontados = [];
contexto.RegiaoDeMod = class {
  constructor(id, dono, raiz, perfil) {
    this.id = id;
    this.dono = dono;
    this.raiz = raiz;
    // O orçamento com que foi montado: o perfil do ponto (`PERFIS_DE_RENDER`).
    this.perfil = perfil;
    renderersMontados.push(this);
  }
  aplicar() { return 0; }
  soltar() { soltouRenderer += 1; }
  calarSonsForaDaTela() {}
};
contexto.PERFIS_DE_RENDER = { cartao: {}, superficie: {} };
contexto.elemento = (tag) => new No(tag, "");
// O `mod` vai junto: é com o hash dele que o dono de verdade pede a mídia.
contexto.donoDaRegiao = (mod, instancia) => ({ instancia, mod, falar() {} });

{
  const base = ler("base.js");
  // Desde `aplicarAContribuicao`, que `montarContribuicao` chama e que vem logo
  // antes dela (tarefa 5, Step 6: a mesma aplicação na montagem e no
  // `atualizar`).
  const inicio = base.indexOf("function aplicarAContribuicao(");
  const fim = base.indexOf("/**\n * As ações que os MODs acrescentaram", inicio);
  confere("R4b · o recorte", inicio >= 0 && fim > inicio
    && base.slice(inicio, fim).includes("function montarContribuicao("),
  "`montarContribuicao` mudou de forma e o recorte não a achou");
  vm.runInContext(base.slice(inicio, fim), contexto);

  const tela = ler("tela-sessao.js");
  const deCartao = tela.indexOf("function cartaoDeContribuicao(");
  const fimDoCartao = tela.indexOf("\n}", deCartao) + 2;
  confere("R4b · o recorte", deCartao >= 0, "`cartaoDeContribuicao` mudou de forma e o recorte não a achou");
  vm.runInContext(tela.slice(deCartao, fimDoCartao), contexto);

  const registro = new R();
  const dono = new I("mod/a", "a", 7, {});
  dono.estado = contexto.ESTADOS_DE_MOD.ativa;

  // **Uma contribuição montada, revogada.** Antes: o registro lógico saía e o
  // renderer, o nó e o descartador ficavam retidos até a sessão acabar.
  const { handle } = registro.registrar({ id: "mod/a" }, dono, {
    ponto: "pessoa.cartao", modo: "adicionar", alvo: "12",
    conteudo: [{ forma: "texto", chave: "c", dentro: "oi" }],
  });
  const contribuicao = registro.porHandle.get(handle);
  const no = contexto.montarContribuicao(contribuicao, "12");
  confere("R4b · montar", Boolean(no), "a contribuição com conteúdo não montou nada");
  confere("R4b · montar", dono.recursos.length === 2, `a montagem não se registrou: ${dono.recursos.length} recurso(s)`);

  const antes = soltouRenderer;
  registro.revogar(handle, { id: "mod/a", instancia: dono });
  confere("R4b · revogar", soltouRenderer === antes + 1, "revogar não soltou o renderer da montagem");
  confere("R4b · revogar", dono.recursos.length === 0, `revogar deixou ${dono.recursos.length} recurso(s) retido(s)`);

  // **O hash do pacote, e da instância.** A contribuição guarda só o id do MOD
  // (`mod: mod.id`, no registro), e a montagem pedia a mídia com `{ id }`: sem
  // o `hash`, que `midia_do_mod` e `som_do_mod` exigem, o Tauri recusa o pedido
  // («missing required key hash») e nenhuma mídia de contribuição chega.
  const doCartao = renderersMontados.at(-1)?.dono?.mod;
  confere(
    "R4b · montar",
    doCartao?.id === "mod/a" && doCartao?.hash === dono.hash,
    `a montagem pede a mídia sem o hash do pacote da instância («${dono.hash}»): ${JSON.stringify(doCartao)}`,
  );

  // **Os sons de uma contribuição moram num destino só.** Sem alvo, ela monta
  // um renderer por destino, e cada um pedia os bytes, decodificava e segurava
  // a sua cópia do som — e tocava, todos juntos. O lugar de quem segura os sons
  // é um só para todos os destinos, e começa vago: é ele que `montarSom`, em
  // `mods-regiao.js`, toma no primeiro som que chega.
  {
    const { handle: semAlvo } = registro.registrar({ id: "mod/a" }, dono, {
      ponto: "canal.item", modo: "adicionar",
      conteudo: [{ forma: "midia", chave: "sino", fonte: "som/a.wav", tocando: true }],
    });
    const geral = registro.porHandle.get(semAlvo);
    const antesDosDestinos = renderersMontados.length;
    contexto.montarContribuicao(geral, "1");
    contexto.montarContribuicao(geral, "2");
    const [um, dois] = renderersMontados.slice(antesDosDestinos);
    confere(
      "R4b · um som por contribuição",
      Boolean(um && dois) && um.sonsDaContribuicao?.tomados === false
        && um.sonsDaContribuicao === dois.sonsDaContribuicao,
      "os destinos de uma contribuição sem alvo não dividem o lugar de quem segura os sons, e cada um "
        + "pede, decodifica e toca a sua cópia: "
        + `${JSON.stringify([um?.sonsDaContribuicao, dois?.sonsDaContribuicao])}`,
    );
    registro.revogar(semAlvo, { id: "mod/a", instancia: dono });
  }

  // E mil ciclos **com conteúdo montado** estabilizam.
  const antesDoLaco = dono.recursos.length;
  for (let n = 0; n < 1000; n += 1) {
    const feito = registro.registrar({ id: "mod/a" }, dono, {
      ponto: "canal.item", modo: "adicionar", alvo: "1",
      conteudo: [{ forma: "texto", chave: "c", dentro: `n${n}` }],
    });
    contexto.montarContribuicao(registro.porHandle.get(feito.handle), "1");
    registro.revogar(feito.handle, { id: "mod/a", instancia: dono });
  }
  confere(
    "R4b · mil com desenho",
    dono.recursos.length === antesDoLaco,
    `mil ciclos com conteúdo montado deixaram ${dono.recursos.length} recurso(s) retido(s)`,
  );
  confere("R4b · mil com desenho", registro.porHandle.size === 0, "sobraram contribuições vivas");
}

// --------------- I-1 · o som de quem saiu da tela, pela varredura de base.js

{
  // `calarOsSonsQueSairamDaTela` mora em `base.js`, no recorte que R4b já
  // roda, e é chamada no fim de cada desenho que pode tirar um nó de MOD da
  // tela. Ela pergunta a cada renderer se as mídias de cartão dele desenham
  // agora; os renderers aqui são de mentira e anotam a resposta — o som de
  // cada um é medido em `regiao-do-mod.cjs`.
  //
  // A regra é a de `midiasDoPonto`: uma substituição desenha onde
  // `escolherSubstituicao` a escolhe, e um acréscimo desenha sempre. (A
  // região da API 3 e os cartões dela saíram com ela.)
  const ouvidos = new Map();
  const renderer = (nome) => ({
    calarSonsForaDaTela(cartoesPintam = true) { ouvidos.set(nome, cartoesPintam); },
  });
  const registro = new R();
  const instancia = (id) => {
    const nova = new I(id, id.replace("/", "-"), 7, {});
    nova.estado = contexto.ESTADOS_DE_MOD.ativa;
    return nova;
  };
  const [a, b, c] = ["mod/a", "mod/b", "mod/c"].map(instancia);
  contexto.contribuicoesDosMods = registro;
  contexto.preferenciaConsultadaPara = () => "";
  const montada = (mod, dono, pedido, nome) => {
    const { handle } = registro.registrar({ id: mod }, dono, pedido);
    registro.porHandle.get(handle).montadas = new Map([[pedido.alvo ?? "", { renderer: renderer(nome) }]]);
  };
  montada("mod/a", a, { ponto: "pessoa.cartao", modo: "substituir", alvo: "2", prioridade: 0 }, "a substituição que perdeu");
  montada("mod/b", b, { ponto: "pessoa.cartao", modo: "substituir", alvo: "2", prioridade: 10 }, "a substituição que venceu");
  montada("mod/c", c, { ponto: "canal.item", modo: "adicionar", alvo: "1" }, "o acréscimo");

  const varrer = typeof contexto.calarOsSonsQueSairamDaTela === "function"
    ? contexto.calarOsSonsQueSairamDaTela
    : () => {};
  confere(
    "I-1 · a varredura",
    typeof contexto.calarOsSonsQueSairamDaTela === "function",
    "`calarOsSonsQueSairamDaTela` não existe no recorte de `base.js` que R4b roda: nada cala o som de quem saiu da tela",
  );
  varrer();
  const esperado = {
    "a substituição que perdeu": false,
    "a substituição que venceu": true,
    "o acréscimo": true,
  };
  for (const [nome, desenha] of Object.entries(esperado)) {
    confere(
      "I-1 · a varredura",
      ouvidos.get(nome) === desenha,
      ouvidos.has(nome)
        ? `a varredura disse que as mídias de cartão de «${nome}» ${ouvidos.get(nome) ? "desenham" : "não desenham"}, e o `
          + `certo é o contrário: ${desenha ? "um som na tela seria calado" : "um som que saiu da tela seguiria tocando"}`
        : `a varredura não visitou «${nome}»: um som que saia da tela ali seguiria tocando sem controle à vista`,
    );
  }

  // Com «o SEELE desenha» o cartão, as duas substituições deixam de desenhar.
  contexto.preferenciaConsultadaPara = () => contexto.NATIVO;
  ouvidos.clear();
  varrer();
  confere(
    "I-1 · a varredura",
    ouvidos.get("a substituição que venceu") === false && ouvidos.get("o acréscimo") === true,
    "com «o SEELE desenha» o cartão, a substituição escolhida antes seguiu desenhando para a varredura, ou o "
      + `acréscimo deixou de desenhar: ${JSON.stringify(Object.fromEntries(ouvidos))}`,
  );
  for (const handle of Array.from(registro.porHandle.keys())) registro.revogar(handle);
  delete contexto.preferenciaConsultadaPara;
}

// --------------------- R4c · mil superfícies criadas e descartadas

{
  // A casca e o renderer são de mentira; `constructor` e `descartar` são os do
  // produto. O construtor registrava o descarte na instância e **jogava fora**
  // o retorno, então descartar tirava a janela da tela e deixava a entrada.
  const cascaReal = S.prototype.montarCasca;
  S.prototype.montarCasca = function montarCascaDeTeste() {
    this.raiz = new No("div", "");
    this.corpo = new No("div", "");
  };
  const instancia = new I("mod/s", "s", 7, {});
  instancia.estado = contexto.ESTADOS_DE_MOD.ativa;
  for (let n = 0; n < 1000; n += 1) {
    const superficie = new S("mod/s", { instancia }, { id: String(n), tipo: "pagina" }, {});
    superficie.descartar();
  }
  confere(
    "R4c",
    instancia.recursos.length === 0,
    `mil superfícies criadas e descartadas deixaram ${instancia.recursos.length} registro(s) retido(s)`,
  );

  // E descartar duas vezes não descarta duas vezes nem entra em recursão.
  const so = new S("mod/s", { instancia }, { id: "uma", tipo: "pagina" }, {});
  so.descartar();
  so.descartar();
  confere("R4c", instancia.recursos.length === 0, "descartar duas vezes deixou recurso para trás");
  S.prototype.montarCasca = cascaReal;
}

// ----------- R4d · o que não coube numa contribuição ou num cartão é dito

{
  // **O MOD sabia; o `seele.log`, não.** A contribuição que não coube virava um
  // evento para o MOD, a que lançou virava um `console.warn` da janela — que
  // num app empacotado não é lugar nenhum —, e os cartões recusados voltavam só
  // como o valor do pedido. Cada um tem de chegar a `registrarNoAnfitriao` como
  // aviso e com o id do MOD em campo próprio.
  const anotadas = [];
  const registrarDeAntes = contexto.registrarNoAnfitriao;
  const rendererDeAntes = contexto.RegiaoDeMod;
  contexto.registrarNoAnfitriao = (...argumentos) => anotadas.push(argumentos);
  const dita = (idDoMod, ...pedacos) => anotadas.some(([onde, texto, nivel, modId]) =>
    onde === "recusa-de-mod" && nivel === "aviso" && modId === idDoMod
    && pedacos.every((pedaco) => String(texto).includes(pedaco)));

  const registro = new R();
  const dono = new I("mod/a", "a", 7, {});
  dono.estado = contexto.ESTADOS_DE_MOD.ativa;
  const contribuir = (alvo) => registro.porHandle.get(registro.registrar({ id: "mod/a" }, dono, {
    ponto: "canal.item", modo: "adicionar", alvo,
    conteudo: [{ forma: "texto", chave: "c", dentro: "x" }],
  }).handle);

  // Não coube: o renderer devolve três recusas.
  contexto.RegiaoDeMod = class { aplicar() { return 3; } soltar() {} };
  contexto.montarContribuicao(contribuir("1"), "1");
  confere("R4d · não coube", dita("mod/a", "3 nó(s)", "canal.item"),
    `a contribuição que não coube não chegou ao registro: ${JSON.stringify(anotadas)}`);

  // Lançou ao montar.
  contexto.RegiaoDeMod = class { aplicar() { throw new Error("forma quebrada"); } soltar() {} };
  const no = contexto.montarContribuicao(contribuir("2"), "2");
  confere("R4d · lançou", no === null, "a montagem que lançou devolveu um nó");
  confere("R4d · lançou", dita("mod/a", "canal.item", "forma quebrada"),
    `a montagem que lançou não chegou ao registro: ${JSON.stringify(anotadas)}`);

  // **Um descarte de montagem que lança**, pelo `tirar` de verdade. Era o
  // outro `console.warn` da janela no mesmo caminho: revogar seguia, e o
  // conteúdo que não saiu da tela só era dito a um console que ninguém lê.
  const presa = contribuir("3");
  presa.soltarMontagem = () => { throw new Error("descarte quebrado"); };
  registro.revogar(presa.handle, { id: "mod/a", instancia: dono });
  confere("R4d · não saiu", !registro.porHandle.has(presa.handle),
    "o descarte que lançou impediu a revogação de tirar a contribuição");
  confere("R4d · não saiu", dita("mod/a", "canal.item", "não saiu", "descarte quebrado"),
    `o descarte que lançou não chegou ao registro: ${JSON.stringify(anotadas)}`);
  // **O ponto entre «», como nas outras recusas de contribuição** (pendência
  // 51, item 23): `«ponto» não montou` e `N nó(s) de «ponto»`, em `base.js`.
  // Uma busca pelo ponto entre «» no seele.log achava as outras e não esta.
  confere("R4d · não saiu", dita("mod/a", "o conteúdo de «canal.item» não saiu"),
    "a recusa do descarte escreveu o ponto sem «», ao contrário das outras recusas de contribuição: "
      + `${JSON.stringify(anotadas.filter(([, texto]) => String(texto).includes("não saiu")))}`);

  contexto.RegiaoDeMod = rendererDeAntes;
  contexto.registrarNoAnfitriao = registrarDeAntes;
}

// ------------- o palco da chamada monta com o perfil da superfície

{
  // **`camadas.chamada.palco.perfil`** (a auditoria do `v6.json`, tabela 7): o
  // palco é uma camada sobre a chamada, com o orçamento de uma superfície, e
  // não o de um cartão. A `tela` do palco montava igual no perfil errado, e
  // trocar o perfil da tabela passava. O perfil do texto, o da tabela de
  // pontos, e o com que a montagem de verdade constrói o renderer.
  const v6 = JSON.parse(fs.readFileSync(path.join(__dirname, "../../../api/v6.json"), "utf8"));
  const doTexto = v6.camadas["chamada.palco"].perfil;
  const daTabela = vm.runInContext('PONTOS_DE_CONTRIBUICAO["chamada.palco"].perfil', contexto);
  confere("o palco · o perfil", doTexto === "superficie" && daTabela === doTexto,
    `o v6.json diz «${doTexto}», e a tabela de pontos diz «${daTabela}»`);
  const registro = new R();
  const dono = new I("mod/p", "p", 7, {});
  dono.estado = contexto.ESTADOS_DE_MOD.ativa;
  const { handle } = registro.registrar({ id: "mod/p" }, dono, {
    ponto: "chamada.palco", modo: "substituir",
    conteudo: [{ forma: "tela", chave: "mesa", largura: 10, altura: 10 }],
  });
  const antes = renderersMontados.length;
  contexto.montarContribuicao(registro.porHandle.get(handle), "");
  const montado = renderersMontados.slice(antes).at(-1);
  confere("o palco · o perfil", montado?.perfil === contexto.PERFIS_DE_RENDER.superficie,
    "a contribuição do palco não foi montada com o orçamento da superfície");
  registro.revogar(handle, { id: "mod/p", instancia: dono });
}

// ------------- R3b · a substituição desenha o conteúdo da própria contribuição

{
  const registro = new R();
  const dono = new I("mod/a", "a", 7, {});
  dono.estado = contexto.ESTADOS_DE_MOD.ativa;

  // **Sem `SeeleUI.cartoes`.** É o ponto: um autor que siga o contrato
  // genérico registra `pessoa.cartao/substituir` com `conteudo` e mais nada.
  const { handle } = registro.registrar({ id: "mod/a" }, dono, {
    ponto: "pessoa.cartao", modo: "substituir",
    conteudo: [{ forma: "texto", chave: "c", dentro: "cartão do MOD" }],
  });
  const contribuicao = registro.porHandle.get(handle);

  const cartao = contexto.cartaoDeContribuicao(contribuicao, 12);
  confere(
    "R3b · o contrato genérico",
    Boolean(cartao),
    "um MOD que registrou `pessoa.cartao/substituir` com conteúdo não teve cartão desenhado: "
    + "o caminho ainda depende de `SeeleUI.cartoes`",
  );

  // **Uma montagem por destino.** Um nó só, devolvido para duas pessoas, não
  // aparece nas duas: `append` move, e a segunda linha rouba o nó da primeira.
  const outro = contexto.cartaoDeContribuicao(contribuicao, 13);
  confere("R3b · por destino", Boolean(outro), "a segunda pessoa não recebeu cartão");
  confere(
    "R3b · por destino",
    cartao !== outro,
    "as duas pessoas receberam o mesmo nó: ele só se move de uma linha para a outra",
  );
  // E o mesmo destino, pedido de novo, devolve o mesmo nó — senão a mídia
  // recomeça e o foco sai a cada retrato.
  confere(
    "R3b · por destino",
    contexto.cartaoDeContribuicao(contribuicao, 12) === cartao,
    "pedir o mesmo destino duas vezes montou um nó novo",
  );

  // E revogar solta as duas montagens.
  const antes = soltouRenderer;
  registro.revogar(handle, { id: "mod/a", instancia: dono });
  confere(
    "R3b · a volta",
    soltouRenderer === antes + 2,
    `revogar soltou ${soltouRenderer - antes} montagem(ns) das duas que existiam`,
  );
  confere("R3b · a volta", dono.recursos.length === 0, "revogar deixou montagem retida na instância");

  // **O legado saiu** (a limpeza das APIs 3 a 5): este build só executa a 6,
  // e uma contribuição sem conteúdo não cai mais num cartão de
  // `SeeleUI.cartoes`: a linha fica com o que o produto desenha.
  const semConteudo = { mod: "mod/a", instancia: dono, ponto: "pessoa.cartao", conteudo: null };
  confere(
    "R3b · sem o legado",
    !contexto.cartaoDeContribuicao(semConteudo, 12),
    "uma contribuição sem conteúdo caiu num cartão de `SeeleUI.cartoes`, que saiu com as APIs 3 a 5",
  );
}

// ----------------------------- M1 · a aba DIAGNÓSTICO, no portão (m-6)

/**
 * As partes puras da aba DIAGNÓSTICO, com os arquivos de verdade.
 *
 * Elas só morriam na bancada Playwright (`diagnostico-de-mods.cjs`), que roda
 * num job manual do `ci.yml`: dos 115 mutantes da revisão ampla do Plano 1D,
 * quinze da aba passavam por todos os portões (m-6). Aqui moram as duas peças
 * que não precisam de navegador:
 *
 * - `midiasDoPonto` e a regra dela, `aContribuicaoPinta` (`base.js`), sobre um
 *   registro de verdade e renderers de mentira que só anotam;
 * - `frasesDeQuemPinta` (`camada-mods.js`), sobre as linhas que
 *   `quemPintaCadaPonto` monta — com `fraseDaMidia`, `fraseDaPreferencia` e
 *   `preferenciaConsultadaPara`, os vizinhos que ela lê.
 *
 * E a vizinha da gestão, logo acima da aba na mesma tela: `oQueAGestaoDiz`
 * (`camada-mods.js`), o estado e os botões de cada linha de QUEM DESENHA O QUE,
 * sobre as linhas que `resumo` monta.
 *
 * Um contexto próprio, e não o da bancada: as funções de cima viram globais
 * dele, e as seções seguintes não podem herdá-las.
 *
 * O desenho na página, o aviso que redesenha a aba e o modo de desenvolvedor
 * continuam medidos no Chromium.
 */
function abaDoDiagnostico() {
  const aba = vm.createContext({
    console,
    queueMicrotask: () => {},
    geracaoDaSessao: 7,
    modsCarregados: new Map(),
  });
  vm.runInContext(`${ler("mods-runtime.js")}\n${ler("mods-contribuicoes.js")}\n`, aba);
  const base = ler("base.js");
  const camada = ler("camada-mods.js");
  const recortes = [
    [base, "function midiasDoPonto(", "/**\n * Cala o som de MOD que um desenho tirou da tela."],
    [camada, "function preferenciaConsultadaPara(", "/**\n * Escolhe quem apresenta um ponto"],
    [camada, "function oQueAGestaoDiz(", "/**\n * O valor que quer dizer «o SEELE desenha, e nenhum MOD»."],
    [camada, "const APRESENTACAO_NATIVA = ", "/** Uma linha: o nome em palavra"],
  ];
  for (const [fonte, de, ate] of recortes) {
    const inicio = fonte.indexOf(de);
    const fim = fonte.indexOf(ate, inicio);
    confere(
      "M1 · a aba · o recorte",
      inicio >= 0 && fim > inicio,
      `«${de}» deixou de vir antes de «${ate.replace(/\n/g, " ")}», e o recorte não a achou`,
    );
    if (inicio >= 0 && fim > inicio) vm.runInContext(fonte.slice(inicio, fim), aba);
  }
  // O que a aba lê da página e dos MODs de pé: a preferência desta máquina e
  // as superfícies de cada instância. O registro é de verdade.
  aba.preferencias = new Map();
  aba.modPreferidoPara = (ponto) => aba.preferencias.get(ponto) ?? "";
  aba.superficiesDosMods = new Map();
  aba.contribuicoesDosMods = vm.runInContext("new RegistroDeContribuicoes()", aba);
  return aba;
}

/** Um renderer de mentira: só as anotações que `midiasDoPonto` lê. */
function anotando(...anotadas) {
  return { midiasAnotadas: () => anotadas.map((anotada) => ({ motivo: "", cartao: false, ...anotada })) };
}

{
  const aba = abaDoDiagnostico();
  const registro = aba.contribuicoesDosMods;
  const linha = (ponto) => aba.quemPintaCadaPonto().find((l) => l.ponto === ponto);
  const frases = (ponto) => aba.frasesDeQuemPinta(linha(ponto)).join(" | ");

  // **A mídia é de quem pinta.** Duas substituições da pessoa 2, cada uma com
  // a sua montagem anotada: conta só a da que vence.
  const venceu = registro.porHandle.get(registro.registrar({ id: "mod/a" }, null, {
    ponto: "pessoa.cartao", modo: "substituir", alvo: "2", prioridade: 10,
  }).handle);
  const perdeu = registro.porHandle.get(registro.registrar({ id: "mod/b" }, null, {
    ponto: "pessoa.cartao", modo: "substituir", alvo: "2", prioridade: 5,
  }).handle);
  venceu.montadas = new Map([["2", { renderer: anotando({ situacao: "pronta" }) }]]);
  perdeu.montadas = new Map([["2", { renderer: anotando({ situacao: "carregando" }, { situacao: "carregando" }) }]]);
  confere(
    "M1 · a aba · a mídia de quem pinta",
    JSON.stringify(aba.midiasDoPonto("pessoa.cartao")) === JSON.stringify({ carregando: 0, pronta: 1, recusada: 0, motivos: [] }),
    "a mídia de quem perdeu a disputa foi contada ao lado da de quem pinta, ou a de quem pinta sumiu: "
      + `${JSON.stringify(aba.midiasDoPonto("pessoa.cartao"))}`,
  );
  // Com mod/b escolhido, a conta troca de lado — a regra é a da tela.
  aba.preferencias.set("pessoa.cartao", "mod/b");
  confere(
    "M1 · a aba · a mídia de quem pinta",
    aba.midiasDoPonto("pessoa.cartao").carregando === 2 && aba.midiasDoPonto("pessoa.cartao").pronta === 0,
    `com mod/b escolhido, a mídia contada não passou a ser a dele: ${JSON.stringify(aba.midiasDoPonto("pessoa.cartao"))}`,
  );
  aba.preferencias.clear();

  // **O retrato de avatar conta**, quando a contribuição dele pinta.
  const avatar = registro.porHandle.get(registro.registrar({ id: "mod/a" }, null, {
    ponto: "pessoa.avatar", modo: "substituir", alvo: "1", conteudo: { doServidor: { canal: 1 } },
  }).handle);
  avatar.retrato = { situacao: "recusada", motivo: `${"m".repeat(199)}😀${"n".repeat(20)}` };
  const doAvatar = aba.midiasDoPonto("pessoa.avatar");
  confere(
    "M1 · a aba · o retrato",
    doAvatar.recusada === 1,
    `o retrato recusado de um avatar não foi contado em «pessoa.avatar»: ${JSON.stringify(doAvatar)}`,
  );
  // E o motivo é cortado em 200 pontos de código, e não por índice: um corte
  // por índice parte o par substituto da posição 200 ao meio.
  confere(
    "M1 · a aba · o retrato",
    doAvatar.motivos[0] === `mod/a: ${"m".repeat(199)}😀`,
    `o motivo do retrato não saiu cortado em 200 pontos de código inteiros: ${JSON.stringify(doAvatar.motivos[0]?.slice(-4))}`,
  );

  // **A preferência que o avatar consulta**: a dele, e na falta dela a do cartão.
  aba.preferencias.set("pessoa.cartao", "mod/a");
  confere(
    "M1 · a aba · a herança",
    aba.preferenciaConsultadaPara("pessoa.avatar") === "mod/a",
    `sem escolha própria, o avatar não herdou a escolha do cartão: «${aba.preferenciaConsultadaPara("pessoa.avatar")}»`,
  );
  aba.preferencias.set("pessoa.avatar", vm.runInContext("NATIVO", aba));
  confere(
    "M1 · a aba · a herança",
    aba.preferenciaConsultadaPara("pessoa.avatar") === ":nativo",
    `com escolha própria, o avatar seguiu a do cartão: «${aba.preferenciaConsultadaPara("pessoa.avatar")}»`,
  );
  // **E a capa, a mesma herança** (`capa.preferencia`).
  confere(
    "M1 · a aba · a herança",
    aba.preferenciaConsultadaPara("pessoa.capa") === "mod/a",
    `sem escolha própria, a capa não herdou a escolha do cartão: «${aba.preferenciaConsultadaPara("pessoa.capa")}»`,
  );
  aba.preferencias.set("pessoa.capa", vm.runInContext("NATIVO", aba));
  confere(
    "M1 · a aba · a herança",
    aba.preferenciaConsultadaPara("pessoa.capa") === ":nativo",
    `com escolha própria, a capa seguiu a do cartão: «${aba.preferenciaConsultadaPara("pessoa.capa")}»`,
  );
  aba.preferencias.clear();

  // **As frases.** Um lugar vazio, e a mídia de um lugar que tem quem pinte e
  // não tem mídia nenhuma.
  confere(
    "M1 · a aba · as frases",
    frases("compositor.ferramentas") === "nenhum MOD usa este lugar agora",
    `um lugar sem MOD não disse que está vazio: ${frases("compositor.ferramentas")}`,
  );
  registro.registrar({ id: "mod/a" }, null, { ponto: "canal.item", modo: "adicionar" });
  confere(
    "M1 · a aba · as frases",
    frases("canal.item") === "acrescentam: mod/a | mídia: nenhuma em uso agora",
    `um lugar com quem acrescenta e sem mídia não disse que nenhuma está em uso: ${frases("canal.item")}`,
  );

  // **O caso parcial**: mod/b escolhido desenha a pessoa 2, e a pessoa 3, que
  // só mod/a declarou, fica com o SEELE.
  registro.registrar({ id: "mod/a" }, null, {
    ponto: "pessoa.avatar", modo: "substituir", alvo: "3", conteudo: { doServidor: { canal: 1 } },
  });
  registro.registrar({ id: "mod/b" }, null, {
    ponto: "pessoa.avatar", modo: "substituir", alvo: "2", conteudo: { doServidor: { canal: 1 } },
  });
  aba.preferencias.set("pessoa.avatar", "mod/b");
  confere(
    "M1 · a aba · as frases",
    frases("pessoa.avatar").startsWith("desenhado por mod/b, para quem declarou; o SEELE desenha os outros |"),
    `com mod/b escolhido, a pessoa que só mod/a declarou é desenhada pelo SEELE, e a linha não disse: ${frases("pessoa.avatar")}`,
  );
  aba.preferencias.clear();
}

// **As páginas de cada MOD** (pendência 49, item 5; m-2 da revisão ampla do
// Plano 1D). A aba contava só a mídia dos onze lugares, e as superfícies de
// cada MOD anotavam e ninguém lia: uma trilha recusada numa página chegava ao
// `seele.log` e ao MOD, e não à aba. (A faixa da API 3, a região, somava aqui
// também, e saiu com ela.)
{
  const caso = "M1 · a aba · as páginas";
  const aba = abaDoDiagnostico();
  const somar = typeof aba.midiasDasPaginas === "function" ? aba.midiasDasPaginas : null;
  confere(
    caso,
    somar !== null && typeof aba.frasesDaMidiaDosMods === "function",
    "`midiasDasPaginas` (`base.js`) ou `frasesDaMidiaDosMods` (`camada-mods.js`) não estão nos recortes "
      + "da aba: ela continua sem ler a mídia das páginas",
  );
  if (somar) {
    confere(
      caso,
      JSON.stringify(aba.frasesDaMidiaDosMods()) === JSON.stringify(["nenhum MOD tem página de pé agora"]),
      `sem MOD de pé, o bloco não disse que não há nada: ${JSON.stringify(aba.frasesDaMidiaDosMods())}`,
    );
    // Duas instâncias de mod/x, como no produto — as superfícies são
    // guardadas por instância, e a soma é por id —, uma com um som e um
    // retrato recusados, a outra com um fundo recusado; e mod/y sem mídia.
    aba.superficiesDosMods.set({}, {
      id: "mod/x",
      porChave: new Map([["painel", { renderer: anotando(
        { situacao: "recusada", motivo: "o som não decodificou" },
        { situacao: "recusada", motivo: "o retrato passou do teto" },
      ) }]]),
    });
    aba.superficiesDosMods.set({}, {
      id: "mod/x",
      porChave: new Map([["mesa", { renderer: anotando({ situacao: "recusada", motivo: "o fundo não é imagem" }) }]]),
    });
    aba.superficiesDosMods.set({}, { id: "mod/y", porChave: new Map([["capa", { renderer: anotando() }]]) });
    const porMod = somar(aba.superficiesDosMods);
    const deX = porMod.get("mod/x");
    confere(
      caso,
      deX?.recusada === 3 && deX.carregando === 0 && deX.pronta === 0
        && JSON.stringify(deX.motivos) === JSON.stringify([
          "o som não decodificou", "o retrato passou do teto", "o fundo não é imagem",
        ]),
      "as duas instâncias de mod/x não somaram as três recusas com os três motivos — uma das páginas ficou de "
        + `fora: ${JSON.stringify(deX)}`,
    );
    confere(
      caso,
      JSON.stringify(porMod.get("mod/y")) === JSON.stringify({ carregando: 0, pronta: 0, recusada: 0, motivos: [] }),
      `a mídia de mod/x foi contada para mod/y, ou mod/y sumiu: ${JSON.stringify(porMod.get("mod/y"))}`,
    );
    const frases = aba.frasesDaMidiaDosMods();
    confere(
      caso,
      JSON.stringify(frases) === JSON.stringify([
        "as páginas de mod/x: 3 recusadas — o som não decodificou; o retrato passou do teto; o fundo não é imagem",
        "as páginas de mod/y: nenhuma mídia montada",
      ]),
      `o bloco não disse a mídia de cada MOD com os motivos: ${JSON.stringify(frases)}`,
    );
  }
}

// **«Não está de pé» só de quem não está de pé** (I-3 da revisão ampla do
// Plano 1D). `ausente`, na disputa, quer dizer só que o escolhido não tem
// candidata neste lugar; a aba o traduzia sempre por «não está de pé», e quem
// escreve o MOD ia depurar uma carga que não falhou. Os três casos em que o
// escolhido está carregado, cada um num registro novo:
{
  const caso = "M1 · a aba · o escolhido";
  const cenario = (montar) => {
    const aba = abaDoDiagnostico();
    for (const id of ["mod/a", "mod/b"]) aba.modsCarregados.set(id, {});
    montar(aba.contribuicoesDosMods, aba.preferencias);
    return (ponto) => aba.frasesDeQuemPinta(aba.quemPintaCadaPonto().find((l) => l.ponto === ponto)).join(" | ");
  };
  const avatar = (alvo) => ({ ponto: "pessoa.avatar", modo: "substituir", alvo, conteudo: { doServidor: { canal: 1 } } });

  // A herança: o avatar segue a escolha do cartão, e o escolhido do cartão
  // desenha cartões e não avatares.
  const heranca = cenario((registro, preferencias) => {
    registro.registrar({ id: "mod/a" }, null, { ponto: "pessoa.cartao", modo: "substituir" });
    registro.registrar({ id: "mod/b" }, null, avatar("1"));
    registro.registrar({ id: "mod/b" }, null, avatar("2"));
    preferencias.set("pessoa.cartao", "mod/a");
  });
  confere(
    caso,
    heranca("pessoa.avatar").startsWith(
      "a escolha de «O cartão de cada pessoa» é mod/a, que não desenha avatares: o SEELE desenha |",
    ) && heranca("pessoa.cartao").startsWith("desenhado por mod/a |"),
    "o avatar herdou a escolha do cartão, mod/a, que está de pé e não desenha avatares, e a aba não disse isso — "
      + `ou disse que mod/a não está de pé: avatar «${heranca("pessoa.avatar")}», cartão «${heranca("pessoa.cartao")}»`,
  );

  // O escolhido revogou a contribuição e continua rodando.
  const revogado = cenario((registro, preferencias) => {
    const { handle } = registro.registrar({ id: "mod/a" }, null, { ponto: "pessoa.cartao", modo: "substituir" });
    registro.registrar({ id: "mod/b" }, null, { ponto: "pessoa.cartao", modo: "substituir" });
    preferencias.set("pessoa.cartao", "mod/a");
    registro.revogar(handle, null);
  });
  confere(
    caso,
    revogado("pessoa.cartao").startsWith("você escolheu mod/a, que está de pé e não substitui este lugar: o SEELE desenha |"),
    `mod/a, escolhido, revogou o cartão e continua de pé, e a aba disse outra coisa: ${revogado("pessoa.cartao")}`,
  );

  // O escolhido só acrescenta naquele lugar.
  const acrescenta = cenario((registro, preferencias) => {
    registro.registrar({ id: "mod/a" }, null, { ponto: "pessoa.cartao", modo: "substituir", alvo: "2" });
    registro.registrar({ id: "mod/b" }, null, { ponto: "pessoa.cartao", modo: "adicionar", alvo: "2" });
    preferencias.set("pessoa.cartao", "mod/b");
  });
  confere(
    caso,
    acrescenta("pessoa.cartao").startsWith(
      "você escolheu mod/b, que está de pé e não substitui este lugar: o SEELE desenha | acrescentam: mod/b |",
    ),
    `mod/b, escolhido, só acrescenta ao cartão e está de pé, e a aba disse outra coisa: ${acrescenta("pessoa.cartao")}`,
  );

  // E quem não está carregado continua dito como quem não está de pé.
  const desligado = cenario((registro, preferencias) => {
    registro.registrar({ id: "mod/a" }, null, { ponto: "pessoa.cartao", modo: "substituir" });
    preferencias.set("pessoa.cartao", "mod/z");
  });
  confere(
    caso,
    desligado("pessoa.cartao").startsWith("você escolheu mod/z, e ele não está de pé agora: o SEELE desenha |"),
    `mod/z, escolhido, não está carregado, e a aba não disse que ele não está de pé: ${desligado("pessoa.cartao")}`,
  );
}

// **O escolhido que desenha só quem declarou, na gestão** (D-I1 da revisão do
// Lote Diagnóstico da correção ampla do Plano 1D). `resumo` decidia só pela
// disputa do alvo vazio, e `quemPinta` soma os alvos. mod/a substitui o cartão
// de todos, mod/b só o da pessoa 2, e esta máquina escolheu mod/b: no alvo
// vazio ele não tem candidata, e a disputa diz `ausente` — mas ele desenha a
// pessoa 2. A gestão dizia «você escolheu mod/b, que está de pé e não substitui
// este lugar — o SEELE desenha», com a aba logo abaixo dizendo «desenhado por
// mod/b, para quem declarou», e `nativa` escondia «USAR APRESENTAÇÃO DO SEELE»
// e a nota de quem perdeu enquanto mod/b desenhava a pessoa 2.
{
  const caso = "M1 · a gestão · o caso parcial";
  const registro = new R();
  registro.registrar({ id: "mod/a" }, null, { ponto: "pessoa.cartao", modo: "substituir" });
  registro.registrar({ id: "mod/b" }, null, { ponto: "pessoa.cartao", modo: "substituir", alvo: "2" });
  registro.registrar({ id: "mod/c" }, null, { ponto: "pessoa.cartao", modo: "adicionar", alvo: "2" });
  const deDe = (id) => ["mod/a", "mod/b", "mod/c"].includes(id);
  const naGestao = (escolha) => registro.resumo(new Map([["pessoa.cartao", escolha]]), deDe)
    .find((linha) => linha.ponto === "pessoa.cartao");

  const parcial = naGestao("mod/b");
  confere(
    caso,
    parcial.escolhidoNaoSubstitui === "" && parcial.nativa === false && parcial.ausente === "",
    "mod/b, escolhido, desenha a pessoa 2, e a gestão o disse como quem não substitui o lugar, ou deu o lugar ao "
      + `SEELE — e escondeu «USAR APRESENTAÇÃO DO SEELE» e a nota de quem perdeu: ${JSON.stringify(parcial)}`,
  );
  confere(
    caso,
    parcial.escolhido === "mod/b"
      && parcial.oSeeleDesenhaOutros === true
      && parcial.preteridos.join() === "mod/a",
    "mod/b, escolhido, desenha a pessoa 2 e o SEELE desenha as outras, e a gestão não disse isso — ou não disse que "
      + `mod/a perdeu: ${JSON.stringify(parcial)}`,
  );
  // E a gestão diz o que a aba diz: as duas leem o mesmo registro e a mesma
  // escolha, e uma não pode desmentir a outra. Sem ninguém no cartão de
  // todos, a gestão diz a escolha gravada, e o caso parcial dela continua
  // sendo o da aba — o bloco «a escolha desta máquina», abaixo.
  const naAba = registro.quemPinta("pessoa.cartao", "mod/b", deDe);
  confere(
    caso,
    naAba.substitui.join() === parcial.escolhido && naAba.oSeeleDesenhaOutros === parcial.oSeeleDesenhaOutros,
    `a gestão e a aba DIAGNÓSTICO disseram coisas diferentes do mesmo cartão: gestão ${JSON.stringify(parcial)}, `
      + `aba ${JSON.stringify(naAba)}`,
  );

  // Os controles: o caso parcial é só de quem substitui algum alvo. Quem só
  // acrescenta continua «não substitui este lugar», quem não está carregado
  // continua «não está de pé», e o automático e o SEELE escolhido não sobram.
  const acrescenta = naGestao("mod/c");
  confere(
    caso,
    acrescenta.escolhidoNaoSubstitui === "mod/c" && acrescenta.nativa === true
      && acrescenta.escolhido === "" && acrescenta.oSeeleDesenhaOutros === false,
    `mod/c, escolhido, só acrescenta ao cartão, e a gestão o disse como quem desenha alguém: ${JSON.stringify(acrescenta)}`,
  );
  const desligado = naGestao("mod/z");
  confere(
    caso,
    desligado.ausente === "mod/z" && desligado.nativa === true && desligado.oSeeleDesenhaOutros === false,
    `mod/z, escolhido, não está carregado nem registrou nada, e a gestão disse outra coisa: ${JSON.stringify(desligado)}`,
  );
  const semSobra = { "o automático": naGestao(""), "o SEELE escolhido": naGestao(NATIVO) };
  confere(
    caso,
    Object.values(semSobra).every((linha) => linha.oSeeleDesenhaOutros === false),
    "a gestão disse que o SEELE desenha os outros sem caso parcial: "
      + Object.entries(semSobra).map(([nome, linha]) => `${nome} ${JSON.stringify(linha)}`).join(", "),
  );
}

// **A gestão diz a escolha desta máquina, e não oferece o que já vale** (N-m1 e
// D-m1 da revisão do Lote Diagnóstico da correção ampla do Plano 1D). Só
// substituições por pessoa, como sempre no avatar: ninguém substitui o cartão
// de todos, e a disputa do alvo vazio sai antes de olhar a escolha. A linha da
// gestão era só a dessa disputa: dizia «4 contribuição(ões)» com qualquer
// escolha, e oferecia de novo o que já vale — «USAR mod/b» com mod/b escolhido,
// «USAR APRESENTAÇÃO DO SEELE» com o SEELE escolhido. A escolha dita e os
// botões que somem vêm da escolha gravada do ponto; a disputa e quem pinta não
// mudam.
{
  const caso = "M1 · a gestão · a escolha desta máquina";
  /** Um registro novo; devolve, para cada escolha, a linha de `resumo`, o que a gestão diz e a linha da aba. */
  const montar = (registrar) => {
    const aba = abaDoDiagnostico();
    for (const id of ["mod/b", "mod/c", "mod/d", "mod/e"]) aba.modsCarregados.set(id, {});
    registrar(aba.contribuicoesDosMods);
    return (escolha) => {
      const linha = aba.contribuicoesDosMods
        .resumo(new Map([["pessoa.cartao", escolha]]), (id) => aba.modsCarregados.has(id))
        .find((l) => l.ponto === "pessoa.cartao");
      const { estado, botoes } = aba.oQueAGestaoDiz(linha, escolha);
      aba.preferencias.set("pessoa.cartao", escolha);
      const naAba = aba.quemPintaCadaPonto().find((l) => l.ponto === "pessoa.cartao");
      aba.preferencias.clear();
      return { linha, estado, botoes: botoes.map((b) => b.rotulo), naAba };
    };
  };
  const cartao = (alvo, modo) => ({ ponto: "pessoa.cartao", modo, alvo });
  // mod/b substitui o cartão da pessoa 2 e mod/e o da 3; mod/c acrescenta ao
  // da 2, e mod/d ao da 3.
  const naGestao = montar((registro) => {
    registro.registrar({ id: "mod/b" }, null, cartao("2", "substituir"));
    registro.registrar({ id: "mod/c" }, null, cartao("2", "adicionar"));
    registro.registrar({ id: "mod/e" }, null, cartao("3", "substituir"));
    registro.registrar({ id: "mod/d" }, null, cartao("3", "adicionar"));
  });

  // Sem escolha gravada, nada muda: a contagem, «usar» para quem desenha o
  // cartão, e o SEELE; voltar ao automático não, porque já se está nele.
  // **Quem só acrescenta não ganha «usar»**: mod/c e mod/d não desenham o
  // cartão, e escolhê-los faria o SEELE desenhar. (Na API 3, o mod/d que dava
  // cartões por `SeeleUI.cartoes` ganhava; o caminho saiu com ela.)
  const automatico = naGestao("");
  confere(
    caso,
    automatico.estado === "4 contribuição(ões) de mod/b, mod/c, mod/e, mod/d"
      && automatico.botoes.join() === "USAR mod/b,USAR mod/e,USAR APRESENTAÇÃO DO SEELE",
    `sem escolha nesta máquina, a linha da gestão mudou: «${automatico.estado}», ${automatico.botoes.join(", ")}`,
  );

  // mod/b escolhido desenha a pessoa 2; a 3, que só mod/e declarou, fica com o
  // SEELE — e a gestão diz o que a aba, logo abaixo, diz.
  const doB = naGestao("mod/b");
  confere(
    caso,
    doB.estado === "apresentado por mod/b, para quem declarou; o SEELE desenha os outros",
    `com mod/b escolhido, a gestão não disse a escolha desta máquina: «${doB.estado}»`,
  );
  confere(
    caso,
    !doB.botoes.includes("USAR mod/b")
      && doB.botoes.includes("USAR mod/e")
      && doB.botoes.includes("USAR APRESENTAÇÃO DO SEELE")
      && doB.botoes.includes("DECIDIR AUTOMATICAMENTE"),
    "com mod/b escolhido, a gestão ofereceu de novo «USAR mod/b», que já vale, ou deixou de oferecer as outras "
      + `escolhas: ${doB.botoes.join(", ")}`,
  );
  confere(
    caso,
    doB.linha.oSeeleDesenhaOutros === doB.naAba.oSeeleDesenhaOutros && doB.naAba.substitui.join() === doB.linha.escolhido,
    `com mod/b escolhido, a gestão e a aba DIAGNÓSTICO disseram coisas diferentes do mesmo cartão: gestão `
      + `${JSON.stringify(doB.linha)}, aba ${JSON.stringify(doB.naAba)}`,
  );

  // O SEELE escolhido: nenhum MOD substitui, e o botão do SEELE some.
  const doSeele = naGestao(NATIVO);
  confere(
    caso,
    doSeele.estado === "o SEELE desenha; nenhum MOD substitui este lugar",
    `com o SEELE escolhido, a gestão não disse a escolha desta máquina: «${doSeele.estado}»`,
  );
  confere(
    caso,
    !doSeele.botoes.includes("USAR APRESENTAÇÃO DO SEELE")
      && doSeele.botoes.includes("DECIDIR AUTOMATICAMENTE")
      && doSeele.botoes.includes("USAR mod/b"),
    "com o SEELE escolhido, a gestão ofereceu de novo «USAR APRESENTAÇÃO DO SEELE», que já vale, ou deixou de "
      + `oferecer as outras escolhas: ${doSeele.botoes.join(", ")}`,
  );

  // mod/d escolhido, que só acrescenta: o SEELE desenha, e a gestão oferece o
  // SEELE e o automático — e nenhum «usar» para ele.
  const doD = naGestao("mod/d");
  confere(
    caso,
    doD.estado === "você escolheu mod/d, que está de pé e não substitui este lugar — o SEELE desenha",
    `com mod/d escolhido, a gestão não disse a escolha desta máquina: «${doD.estado}»`,
  );
  confere(
    caso,
    !doD.botoes.includes("USAR mod/d")
      && doD.botoes.includes("USAR APRESENTAÇÃO DO SEELE")
      && doD.botoes.includes("DECIDIR AUTOMATICAMENTE"),
    "com mod/d escolhido, a gestão ofereceu «USAR mod/d», que só acrescenta, ou deixou de oferecer "
      + `as outras escolhas: ${doD.botoes.join(", ")}`,
  );

  // Quem não está carregado continua dito como quem não está de pé.
  const doZ = naGestao("mod/z");
  confere(
    caso,
    doZ.estado === "você escolheu mod/z, e ele não está de pé agora — o SEELE desenha",
    `com mod/z escolhido, que não está carregado, a gestão não disse a escolha desta máquina: «${doZ.estado}»`,
  );

  // Sozinho, mod/b desenha a pessoa 2, e ninguém mais pediu o cartão: a aba
  // diz «desenhado por mod/b», e a gestão não acrescenta que o SEELE desenha
  // os outros — as duas, na mesma tela, não se desmentem.
  const sozinho = montar((registro) => registro.registrar({ id: "mod/b" }, null, cartao("2", "substituir")))("mod/b");
  confere(
    caso,
    sozinho.estado === "apresentado por mod/b"
      && sozinho.linha.oSeeleDesenhaOutros === sozinho.naAba.oSeeleDesenhaOutros,
    `com mod/b escolhido e sozinho no cartão, a gestão disse outra coisa que a aba: «${sozinho.estado}», `
      + `aba ${JSON.stringify(sozinho.naAba)}`,
  );

  // E o SEELE só some dos botões quando é ele o escolhido. Com alguém no
  // cartão de todos e mod/z escolhido, o SEELE desenha porque mod/z não está
  // de pé — e escolher o SEELE muda a escolha: quando mod/z voltar, quem
  // desenha continua sendo o SEELE.
  const comTodos = montar((registro) => registro.registrar({ id: "mod/b" }, null, {
    ponto: "pessoa.cartao", modo: "substituir",
  }))("mod/z");
  confere(
    caso,
    comTodos.estado === "você escolheu mod/z, e ele não está de pé agora — o SEELE desenha"
      && comTodos.botoes.includes("USAR APRESENTAÇÃO DO SEELE"),
    "com mod/z escolhido e fora de pé, a gestão escondeu «USAR APRESENTAÇÃO DO SEELE», que mudaria a escolha: "
      + `«${comTodos.estado}», ${comTodos.botoes.join(", ")}`,
  );
}

// **No automático misto, a gestão diz todos os que desenham** (pendência 49,
// item 9; Docs-m2 do 1D). mod/a substitui o cartão de todos; mod/b, com mais
// prioridade, o da pessoa 2; mod/e, com menos, o da pessoa 3, e perde para
// mod/a. Sem escolha nesta máquina, a gestão lia só a disputa do alvo vazio e
// dizia «apresentado por mod/a (escolha automática)», sem nota, enquanto mod/b
// desenhava a pessoa 2 — e a aba DIAGNÓSTICO, logo abaixo, dizia a verdade.
{
  const caso = "M1 · a gestão · o automático misto";
  const aba = abaDoDiagnostico();
  const registro = aba.contribuicoesDosMods;
  for (const id of ["mod/a", "mod/b", "mod/e"]) aba.modsCarregados.set(id, {});
  registro.registrar({ id: "mod/a" }, null, { ponto: "pessoa.cartao", modo: "substituir" });
  registro.registrar({ id: "mod/b" }, null, { ponto: "pessoa.cartao", modo: "substituir", alvo: "2", prioridade: 10 });
  registro.registrar({ id: "mod/e" }, null, { ponto: "pessoa.cartao", modo: "substituir", alvo: "3", prioridade: -5 });
  const deDe = (id) => aba.modsCarregados.has(id);
  const linha = registro.resumo(new Map([["pessoa.cartao", ""]]), deDe).find((l) => l.ponto === "pessoa.cartao");
  const { estado, nota, botoes } = aba.oQueAGestaoDiz(linha, "");

  // O comando da pendência: quem desenha a pessoa 2 é mod/b, e a 3, mod/a.
  confere(
    caso,
    registro.escolherSubstituicao("pessoa.cartao", "2", "").escolhida?.mod === "mod/b"
      && registro.escolherSubstituicao("pessoa.cartao", "3", "").escolhida?.mod === "mod/a",
    "o registro deixou de dar a pessoa 2 a mod/b e a 3 a mod/a, e este caso não mede mais o automático misto",
  );
  confere(
    caso,
    estado === "automática: mod/a para todos, mod/b para quem declarou",
    "no automático, mod/a desenha o cartão de todos e mod/b o da pessoa 2, e a gestão nomeou um MOD só — quem "
      + `escreve mod/b lê que ele não desenha nada: «${estado}»`,
  );
  confere(
    caso,
    nota === "Também pediram este lugar e não o receberam: mod/e. Escolha abaixo qual deve desenhar.",
    "mod/e pediu o cartão da pessoa 3 e perdeu para mod/a, e a gestão não o disse entre quem pediu e não recebeu: "
      + `«${nota}»`,
  );
  confere(
    caso,
    botoes.map((b) => b.rotulo).join() === "USAR mod/b,USAR mod/e,USAR APRESENTAÇÃO DO SEELE",
    `no automático misto, a gestão mudou o que oferece: ${botoes.map((b) => b.rotulo).join(", ")}`,
  );
  // E a gestão diz o que a aba diz, nos dois sentidos: quem desenha algum alvo
  // é o mesmo conjunto; quem a aba dá como sem alvo nenhum está na nota; e
  // ninguém da nota desenha — um MOD não é dito como quem desenha e como quem
  // não recebeu ao mesmo tempo.
  const concordam = (umaLinha, naAba) => {
    const desenham = [umaLinha.escolhido, ...(umaLinha.tambemDesenham ?? [])];
    return desenham.sort().join() === [...naAba.substitui].sort().join()
      && naAba.perderam.every((id) => umaLinha.preteridos.includes(id))
      && !umaLinha.preteridos.some((id) => desenham.includes(id));
  };
  const naAba = registro.quemPinta("pessoa.cartao", "", deDe);
  confere(
    caso,
    concordam(linha, naAba),
    `a gestão e a aba DIAGNÓSTICO disseram coisas diferentes do mesmo cartão: gestão ${JSON.stringify(linha)}, `
      + `aba ${JSON.stringify(naAba)}`,
  );

  // **Quem perde o lugar de todos e vence o de alguém desenha** (M1 da revisão
  // do K3). mod/c pede o cartão de todos, com prioridade 0, e perde para
  // mod/a; pede também o da pessoa 2, com prioridade 20, e vence. A gestão
  // dizia «automática: mod/a para todos, mod/c para quem declarou» e, logo
  // abaixo, «Também pediram este lugar e não o receberam: mod/c.», com a aba
  // dizendo que ninguém perdeu.
  const dosDois = abaDoDiagnostico();
  dosDois.contribuicoesDosMods.registrar({ id: "mod/a" }, null, { ponto: "pessoa.cartao", modo: "substituir", prioridade: 10 });
  dosDois.contribuicoesDosMods.registrar({ id: "mod/c" }, null, { ponto: "pessoa.cartao", modo: "substituir" });
  dosDois.contribuicoesDosMods.registrar({ id: "mod/c" }, null, { ponto: "pessoa.cartao", modo: "substituir", alvo: "2", prioridade: 20 });
  const deC = dosDois.contribuicoesDosMods.resumo(new Map([["pessoa.cartao", ""]]), () => true)
    .find((l) => l.ponto === "pessoa.cartao");
  const disseDeC = dosDois.oQueAGestaoDiz(deC, "");
  confere(
    caso,
    disseDeC.estado === "automática: mod/a para todos, mod/c para quem declarou" && disseDeC.nota === "",
    "mod/c perdeu o cartão de todos e desenha o da pessoa 2, e a gestão o disse como quem desenha e como quem não "
      + `recebeu: «${disseDeC.estado}», nota «${disseDeC.nota}»`,
  );
  const deCNaAba = dosDois.contribuicoesDosMods.quemPinta("pessoa.cartao", "", () => true);
  confere(
    caso,
    concordam(deC, deCNaAba),
    `com mod/c perdendo o cartão de todos e vencendo o da pessoa 2, a gestão e a aba se desmentem: gestão `
      + `${JSON.stringify(deC)}, aba ${JSON.stringify(deCNaAba)}`,
  );

  // Os controles: sem ninguém noutro alvo, a frase continua a de um MOD só.
  // Com mod/a escolhido, ou o SEELE, o automático misto não se aplica.
  const sozinho = abaDoDiagnostico();
  sozinho.contribuicoesDosMods.registrar({ id: "mod/a" }, null, { ponto: "pessoa.cartao", modo: "substituir" });
  sozinho.contribuicoesDosMods.registrar({ id: "mod/c" }, null, { ponto: "pessoa.cartao", modo: "adicionar", alvo: "2" });
  const soA = sozinho.contribuicoesDosMods.resumo(new Map([["pessoa.cartao", ""]]), () => true)
    .find((l) => l.ponto === "pessoa.cartao");
  confere(
    caso,
    sozinho.oQueAGestaoDiz(soA, "").estado === "apresentado por mod/a (escolha automática)"
      && (soA.tambemDesenham ?? []).length === 0,
    `com mod/a sozinho a substituir o cartão, a gestão deixou de dizer que ele apresenta: ${JSON.stringify(soA)}`,
  );
  const escolhidos = ["mod/a", NATIVO].map((escolha) => registro
    .resumo(new Map([["pessoa.cartao", escolha]]), deDe).find((l) => l.ponto === "pessoa.cartao"));
  confere(
    caso,
    escolhidos.every((l) => (l.tambemDesenham ?? []).length === 0),
    `com uma escolha gravada, a gestão disse «também desenham» do automático: ${JSON.stringify(escolhidos)}`,
  );
}

// (O tema da API 3 que pintava a sessão, dito na aba como «em vigor (API 3)» —
// I-7 e m-3 da revisão ampla do Plano 1D —, saiu com o corte: na 6, a janela
// só faz prévia, e quem pinta a aparência é o tema do servidor, medido em
// `tema-do-servidor.cjs`.)

// ------------------------------------- R3 · os pontos aceitam o que aplicam

{
  const registro = new R();
  const dono = new I("mod/a", "a", 7, {});
  for (const [ponto, regra] of Object.entries(PONTOS)) {
    for (const modo of regra.modos) {
      let erro = null;
      try {
        registro.registrar({ id: "mod/a" }, dono, {
          ponto, modo, alvo: "1",
          ...(ponto === "pessoa.avatar" || ponto === "pessoa.capa"
            ? { conteudo: { doServidor: { canal: 1, pedido: {} } } }
            : {}),
          // A navegação substituída é uma ordem das listas, e tem forma
          // própria, como o avatar (rascunho §3, tarefa 5).
          ...(ponto === "servidor.navegacao" && modo === "substituir"
            ? { conteudo: { canais: { grupos: [] } } }
            : {}),
          ...(ponto === "pessoas.lista" && modo === "substituir" ? { conteudo: { grupos: [] } } : {}),
        });
      } catch (falha) {
        erro = falha;
      }
      confere("R3 · o que a tabela promete", erro === null, `«${ponto}» recusou «${modo}», que ela anuncia: ${erro?.message}`);
    }
    // **O modo suspenso é recusado pelo nome dele.** «Aceitar registro sem
    // produzir o efeito prometido» é o defeito; recusar em silêncio genérico
    // seria mandar o MOD procurar um erro de ponto que ele não cometeu.
    let mensagem = "";
    try {
      registro.registrar({ id: "mod/a" }, dono, { ponto, modo: "decorar", alvo: "1" });
    } catch (falha) {
      mensagem = falha.message;
    }
    confere(
      "R3 · decorar",
      mensagem.includes("decorar") && mensagem.includes("API 7"),
      `«${ponto}» não explicou a suspensão de «decorar»: ${mensagem || "aceitou o registro"}`,
    );
    // E sugere um modo que o ponto aceita: o avatar e a capa só substituem, e
    // mandá-los usar `adicionar` era mandar o MOD a outra recusa.
    const esperada = regra.modos.includes("adicionar")
      ? "use «adicionar»"
      : `use «${regra.modos.join("» ou «")}»`;
    confere(
      "R3 · decorar sugere o modo do ponto",
      mensagem.includes(esperada) && (regra.modos.includes("adicionar") || !mensagem.includes("«adicionar»")),
      `«${ponto}» (${regra.modos.join(", ")}) não sugeriu ${esperada}: ${mensagem}`,
    );
  }
}

// ------------------- o teto por alvo (tarefa 5 da F3, Step 7; rascunho §12)
//
// O bloqueio C do PERFIS: com cartão, avatar e capa por pessoa, as 128
// contribuições de um MOD davam 42 pessoas, e uma sala vai a 250. Os pontos
// por alvo saem do teto agregado; cada alvo aceita uma contribuição por MOD
// (e por modo) — registrar de novo é `atualizar` —, até um teto de alvos por
// ponto; o agregado fica para os pontos sem alvo.

{
  const registro = new R();
  const dono = new I("mod/perfis", "p", 7, {});
  const recusa = (pedido) => {
    try {
      registro.registrar({ id: "mod/perfis" }, dono, pedido);
      return "";
    } catch (falha) {
      return falha.message;
    }
  };
  let recusadas = 0;
  for (let id = 1; id <= 250; id += 1) {
    if (recusa({ ponto: "pessoa.cartao", modo: "substituir", alvo: String(id) })) recusadas += 1;
    if (recusa({ ponto: "pessoa.avatar", modo: "substituir", alvo: String(id), conteudo: { doServidor: { canal: 1 } } })) {
      recusadas += 1;
    }
  }
  confere("Step 7 · 250 pessoas", recusadas === 0, `${recusadas} das 500 contribuições por pessoa foram recusadas`);

  const segunda = recusa({ ponto: "pessoa.cartao", modo: "substituir", alvo: "1" });
  confere("Step 7 · a segunda para o mesmo alvo", segunda.includes("atualizar") && segunda.includes("pessoa.cartao"),
    `uma segunda contribuição para o mesmo alvo não foi recusada com «use atualizar»: ${segunda || "aceita"}`);
  confere("Step 7 · outro modo, o mesmo alvo",
    recusa({ ponto: "pessoa.cartao", modo: "adicionar", alvo: "1" }) === "",
    "acrescentar à linha de quem o MOD já apresenta foi recusado como se fosse a mesma contribuição");
  // **Dois por alvo, e não três** (decisão da orquestradora, 09/10): com o
  // cartão substituído e a linha acrescida, uma terceira, num modo que já está
  // de pé, é recusada pelo nome.
  {
    let nome = "";
    try {
      registro.registrar({ id: "mod/perfis" }, dono, { ponto: "pessoa.cartao", modo: "adicionar", alvo: "1" });
    } catch (falha) {
      nome = falha.recusa ?? `(sem nome: ${falha.message})`;
    }
    confere("Step 7 · o terceiro no mesmo alvo", nome === "contribuicao-ja-existe",
      `a terceira contribuição para o mesmo alvo, num modo já de pé, veio com ${nome || "(aceita)"}`);
  }

  for (let id = 251; id <= 256; id += 1) recusa({ ponto: "pessoa.cartao", modo: "substituir", alvo: String(id) });
  const alem = recusa({ ponto: "pessoa.cartao", modo: "substituir", alvo: "257" });
  confere("Step 7 · o teto de alvos", alem.includes("256") && alem.includes("pessoa.cartao"),
    `o 257º alvo de «pessoa.cartao» não foi recusado com o número: ${alem || "aceito"}`);

  let entradas = 0;
  for (let i = 0; i < 128; i += 1) if (!recusa({ ponto: "servidor.navegacao", rotulo: `entrada ${i}` })) entradas += 1;
  confere("Step 7 · o agregado é dos pontos sem alvo", entradas === 128,
    `só ${entradas} entradas de navegação couberam: as contribuições por pessoa ainda gastam o teto agregado`);
  const a129 = recusa({ ponto: "servidor.navegacao", rotulo: "entrada 128" });
  confere("Step 7 · a 129ª entrada", a129.includes("128"), `a 129ª entrada não foi recusada com o número: ${a129 || "aceita"}`);

  // Revogar devolve o alvo: o mesmo alvo aceita de novo.
  const umAlvo = [...registro.porHandle.values()].find((c) => c.ponto === "pessoa.cartao" && c.alvo === "2" && c.modo === "substituir");
  registro.revogar(umAlvo.handle, { id: "mod/perfis", instancia: dono });
  confere("Step 7 · revogar devolve o alvo", recusa({ ponto: "pessoa.cartao", modo: "substituir", alvo: "2" }) === "",
    "revogada a contribuição de um alvo, o alvo continuou ocupado");
}

// ------------- as recusas do registro têm nome (tarefa 5 da F3, Step 7b)
//
// Rascunho §7.1, «Como o nome chega»: o `erro` que chega ao MOD da 6 é um nome
// de lista fechada, e a frase fica no `seele.log`. Até o Step 7b, toda recusa
// do registro chegava a ele como `falhou`.

{
  const nomeDa = (fazer) => {
    try {
      fazer();
      return "(aceita)";
    } catch (falha) {
      return falha.recusa ?? `(sem nome: ${falha.message})`;
    }
  };
  const registro = new R();
  const dono = new I("mod/nomes", "n", 7, {});
  const outro = new I("mod/outro", "o", 7, {});
  const registrar = (pedido, quem = dono, id = "mod/nomes") => registro.registrar({ id }, quem, pedido);
  const casos = [
    ["ponto-desconhecido", () => registrar({ ponto: "nao.existe" })],
    ["modo-recusado", () => registrar({ ponto: "pessoa.identidade", modo: "substituir", alvo: "1" })],
    ["modo-recusado", () => registrar({ ponto: "pessoa.cartao", modo: "decorar", alvo: "1" })],
    ["sem-alvo", () => registrar({ ponto: "pessoa.avatar", modo: "substituir", conteudo: { doServidor: { canal: 1 } } })],
    ["conteudo-invalido", () => registrar({ ponto: "pessoa.avatar", modo: "substituir", alvo: "1", conteudo: {} })],
    ["conteudo-invalido", () => registrar({ ponto: "pessoas.lista", modo: "substituir", conteudo: [] })],
    ["conteudo-invalido", () => registrar({ ponto: "servidor.navegacao", modo: "substituir", conteudo: {} })],
  ];
  for (const [nome, fazer] of casos) {
    const visto = nomeDa(fazer);
    confere("Step 7b · o nome", visto === nome, `esperava «${nome}», e veio ${visto}`);
  }
  // **A capa tem a forma do avatar** (`capa.forma`): substituir, por alvo,
  // com `conteudo.doServidor`. Num registro à parte, para uma capa aceita
  // por engano não ocupar o que as conferências de baixo contam.
  {
    const daCapa = new R();
    const capa = (pedido) => () => daCapa.registrar({ id: "mod/capa" }, new I("mod/capa", "c", 7, {}), { ponto: "pessoa.capa", ...pedido });
    for (const [nome, fazer] of [
      ["sem-alvo", capa({ modo: "substituir", conteudo: { doServidor: { canal: 1 } } })],
      ["conteudo-invalido", capa({ modo: "substituir", alvo: "1", conteudo: {} })],
      ["conteudo-invalido", capa({ modo: "substituir", alvo: "1", conteudo: [{ forma: "texto" }] })],
      ["modo-recusado", capa({ modo: "adicionar", alvo: "1", conteudo: { doServidor: { canal: 1 } } })],
      ["(aceita)", capa({ modo: "substituir", alvo: "1", conteudo: { doServidor: { canal: 1 } } })],
    ]) {
      const visto = nomeDa(fazer);
      confere("Step 7b · a capa", visto === nome, `esperava «${nome}», e veio ${visto}`);
    }
  }
  const { handle } = registrar({ ponto: "pessoa.cartao", modo: "substituir", alvo: "1" });
  confere("Step 7b · contribuicao-ja-existe",
    nomeDa(() => registrar({ ponto: "pessoa.cartao", modo: "substituir", alvo: "1" })) === "contribuicao-ja-existe",
    "a segunda contribuição para o mesmo alvo não veio com o nome");
  confere("Step 7b · contribuicao-de-outro-mod",
    nomeDa(() => registro.revogar(handle, { id: "mod/outro", instancia: outro })) === "contribuicao-de-outro-mod"
      && nomeDa(() => registro.atualizar(handle, { id: "mod/outro", instancia: outro }, null)) === "contribuicao-de-outro-mod",
    "revogar ou atualizar o que é de outro MOD não veio com o nome");
  for (let i = 2; i <= 256; i += 1) registrar({ ponto: "pessoa.cartao", modo: "substituir", alvo: String(i) });
  confere("Step 7b · teto-de-alvos",
    nomeDa(() => registrar({ ponto: "pessoa.cartao", modo: "substituir", alvo: "257" })) === "teto-de-alvos",
    "o teto de alvos não veio com o nome");
  // **E com o número** (o PERFIS lê `erro.teto` para contar os de fora): o
  // produto sabe o teto, e o `teto-de-superficies` já o leva.
  const camposDoTetoDeAlvos = (() => {
    try {
      registrar({ ponto: "pessoa.cartao", modo: "substituir", alvo: "257" });
      return "(aceita)";
    } catch (falha) {
      return falha.camposDaRecusa ?? null;
    }
  })();
  confere("Step 7b · teto-de-alvos leva o teto", camposDoTetoDeAlvos?.teto === 256,
    `a recusa do teto de alvos não diz o teto: ${JSON.stringify(camposDoTetoDeAlvos)}`);
  for (let i = 0; i < 128; i += 1) registrar({ ponto: "servidor.navegacao", rotulo: `e${i}` });
  confere("Step 7b · teto-de-contribuicoes",
    nomeDa(() => registrar({ ponto: "servidor.navegacao", rotulo: "e128" })) === "teto-de-contribuicoes",
    "o teto agregado não veio com o nome");
}

// V14: duas instâncias compartilham o destino central e conservam o rascunho.
{
  const montar = S.prototype.montarCasca;
  const focar = S.prototype.focarPrimeiro;
  S.prototype.montarCasca = function () { this.raiz = new No("section"); this.corpo = new No("div"); };
  S.prototype.focarPrimeiro = function () {};
  const palco = porId.get("palco-de-paginas");
  palco.querySelector = () => palco.children.find(no => !no.hidden) ?? null;
  const donoA = new I("pagina/a", "a", 7, {});
  const donoB = new I("pagina/b", "b", 7, {});
  const a = new S("pagina/a", { instancia: donoA }, { id: "perfil", tipo: "pagina" }, { paginas: palco });
  const b = new S("pagina/b", { instancia: donoB }, { id: "mesa", tipo: "pagina" }, { paginas: palco });
  for (const pagina of [a, b]) pagina.aoPalcoMudar = () => { palco.hidden = !palco.querySelector(); };
  a.corpo.rascunho = "não gravado";
  a.abrir(); b.abrir();
  confere("V14 · duas páginas", a.raiz.hidden && !b.raiz.hidden, "as páginas de MODs distintos se empilharam");
  a.mostrar();
  confere("V14 · reabrir", b.raiz.hidden && a.corpo.rascunho === "não gravado", "trocar página perdeu o rascunho ou deixou a anterior visível");
  contexto.$ = id => porId.get(id);
  contexto.atualizar = async () => {};
  contexto.conferirPermissaoDeTela = async () => {};
  contexto.atualizarChamada = async () => {};
  const chamada = ler("tela-chamada.js");
  for (const nome of ["abrirChamada", "fecharChamada"]) {
    const inicio = chamada.indexOf(`function ${nome}(`);
    const fim = chamada.indexOf("\n}", inicio) + 2;
    vm.runInContext((nome === "abrirChamada" ? "async " : "") + chamada.slice(inicio, fim), contexto);
    a.mostrar();
    contexto[nome]();
    confere(`V14 · ${nome}`, palco.hidden && a.raiz.hidden, "o destino nativo continuou coberto");
  }
  b.mostrar(); b.descartar();
  confere("V14 · descarte", palco.hidden && a.raiz.hidden, "descartar ressuscitou uma página antiga");
  a.descartar();
  confere("V14 · limpeza", palco.children.length === 0 && donoA.recursos.length === 0 && donoB.recursos.length === 0, "as páginas retiveram nó ou dono");
  S.prototype.montarCasca = montar;
  S.prototype.focarPrimeiro = focar;
}

// ------------------------ o elo da janela até o Rust, pelo `invoke` de verdade

/**
 * **`registrarNoAnfitriao` e `anotarRecusa` de `base.js`, até o `invoke`.**
 *
 * Todo outro bloco desta bancada troca `registrarNoAnfitriao` por um anotador
 * de mentira, e por isso nenhum lia o que ela repassa ao comando
 * `registrar_da_janela`: tirar o `modId` do `invoke`, ou inverter o `meu()` de
 * `anotarRecusa`, deixava verdes as seis bancadas do `check-runtime` e o
 * `frontend.rs`, cujo guarda de `anotarRecusa` só procura o texto `meu()`
 * (pendência 51, item 13; medido de novo em 05/10/2026, com as duas
 * mutações). Aqui as duas declarações de verdade rodam com um `invoke` de
 * mentira que anota o que recebe.
 *
 * **Num contexto próprio.** O compartilhado tem um `registrarNoAnfitriao`
 * mudo e um `donoDaRegiao` de mentira, e os blocos dele contam com os dois:
 * rodar ali as declarações de verdade as trocaria por baixo deles.
 */
{
  const base = ler("base.js");
  const invocados = [];
  const elo = vm.createContext({
    console,
    geracaoDaSessao: 7,
    modsCarregados: new Map(),
    invoke: (cmd, args) => {
      invocados.push({ cmd, args });
      return Promise.resolve();
    },
    // `donoDaRegiao` a lê ao montar o dono (`midiaMudou`), e sem ela o recorte
    // lançaria antes de medir qualquer coisa.
    avisarQueAMidiaMudou: () => {},
  });
  for (const de of ["function registrarNoAnfitriao(", "function donoDaRegiao("]) {
    const inicio = base.indexOf(de);
    const fim = base.indexOf("\n}\n", inicio);
    confere("o elo · o recorte", inicio >= 0 && fim > inicio,
      `«${de}» mudou de forma, e o recorte até o primeiro «\\n}\\n» não a achou`);
    if (inicio >= 0 && fim > inicio) vm.runInContext(base.slice(inicio, fim + 3), elo);
  }
  /** Os argumentos de cada `registrar_da_janela` que saiu desde `de`. */
  const registrados = (de) => invocados.slice(de).filter((i) => i.cmd === "registrar_da_janela").map((i) => i.args);
  /** O mesmo objeto, campo a campo e sem campo a mais, em qualquer ordem. */
  const comoEsperado = (args, esperado) => args !== null && typeof args === "object"
    && JSON.stringify(Object.keys(args).sort()) === JSON.stringify(Object.keys(esperado).sort())
    && Object.entries(esperado).every(([chave, valor]) => args[chave] === valor);

  if (typeof elo.registrarNoAnfitriao === "function" && typeof elo.donoDaRegiao === "function") {
    const mod = { id: "mod/a", hash: "h" };
    const instancia = { geracao: 7, admite: () => true };
    const dono = elo.donoDaRegiao(mod, instancia);
    // Chamada com `?.`, como a região a chama: sem ela, o caso reprova pela
    // frase dele, e não por um `TypeError` que derrubaria a bancada inteira.
    confere("o elo · anotarRecusa", typeof dono.anotarRecusa === "function",
      "o dono de `base.js` não tem `anotarRecusa`, e `mods-regiao.js` a chama em cada mídia recusada");
    // **A API do pacote, no dono** (tarefa 7, Step 4): as superfícies que um
    // MOD tem dependem dela, e sem ela todo MOD seria tratado como de API 0.
    confere(
      "o elo · a api",
      elo.donoDaRegiao({ id: "mod/x", hash: "h", api: 6 }, instancia).api === 6
        && elo.donoDaRegiao({ id: "mod/x", hash: "h", api: "6" }, instancia).api === 6,
      "o dono de `base.js` não diz a API do pacote, e `tipoDaSuperficie` a lê para saber se há folha ou painel",
    );

    // (1) A instância é a carregada e a sessão é a de pé: uma linha, inteira.
    elo.modsCarregados.set("mod/a", instancia);
    let de = invocados.length;
    dono.anotarRecusa?.("x");
    const esperado = { nivel: "aviso", onde: "recusa-de-mod", oQue: "mod/a: x", modId: "mod/a" };
    confere(
      "o elo · anotarRecusa",
      invocados.length - de === 1 && registrados(de).length === 1 && comoEsperado(registrados(de)[0], esperado),
      `a recusa de mídia não saiu como um \`registrar_da_janela\` só, com ${JSON.stringify(esperado)}: `
        + `${JSON.stringify(invocados.slice(de))}`,
    );

    // (2) Fora da vez: ninguém no mapa, outra instância no lugar desta, ou a
    // sessão que já não é a de pé. Uma mídia que não montou porque a pessoa
    // saiu é o desfecho certo, e não aviso.
    for (const [caso, preparar] of [
      ["sem instância carregada", () => elo.modsCarregados.clear()],
      ["com outra instância no lugar", () => elo.modsCarregados.set("mod/a", { geracao: 7, admite: () => true })],
      ["com a sessão que já não é a de pé", () => {
        elo.modsCarregados.set("mod/a", instancia);
        instancia.admite = () => false;
      }],
    ]) {
      preparar();
      de = invocados.length;
      dono.anotarRecusa?.("x");
      confere(
        "o elo · anotarRecusa fora da vez",
        invocados.length === de,
        `${caso}, a recusa ainda foi ao registro: ${JSON.stringify(invocados.slice(de))}`,
      );
    }
    instancia.admite = () => true;

    // (3) O que `registrarNoAnfitriao` recebe é o que vai ao comando: o nível
    // e o id do MOD também, e não só a frase.
    de = invocados.length;
    elo.registrarNoAnfitriao("o", "t", "erro", "mod/a");
    confere(
      "o elo · registrarNoAnfitriao",
      registrados(de).length === 1
        && comoEsperado(registrados(de)[0], { nivel: "erro", onde: "o", oQue: "t", modId: "mod/a" }),
      "`registrarNoAnfitriao` não repassou ao `registrar_da_janela` o nível e o id do MOD que recebeu: "
        + `${JSON.stringify(invocados.slice(de))}`,
    );

    // (4) **Um substituto UTF-16 solto não atravessa a ponte.** O
    // `JSON.stringify` do `invoke` o escreve `\ud800`, o `serde_json` recusa a
    // cadeia ao ler uma `String`, e o `.catch` de `registrarNoAnfitriao` engole
    // a recusa: a linha some do `seele.log` sem aviso nenhum (pendência 51,
    // item 6). Ele chega como «�», em cada campo de texto; o par inteiro, que é
    // um caractere de verdade, chega como estava.
    de = invocados.length;
    elo.registrarNoAnfitriao("onde\uDC00", "avatar\uD800 😀", "aviso", "mod/a\uD800");
    const saneado = registrados(de)[0];
    confere(
      "o elo · o substituto solto",
      registrados(de).length === 1
        && ["onde", "oQue", "modId"].every((campo) => typeof saneado?.[campo] === "string"
          && saneado[campo].isWellFormed())
        && saneado.oQue === "avatar\uFFFD 😀",
      "um substituto solto chegou ao `registrar_da_janela`, que a ponte recusa inteiro, ou o par de um "
        + `caractere de verdade foi trocado junto: ${JSON.stringify(invocados.slice(de))}`,
    );
    // E sem `modId` o campo vai `null`, e não o texto «null»: o Rust
    // escreveria `mod_id=null`, um MOD que não existe.
    de = invocados.length;
    elo.registrarNoAnfitriao("o", "t");
    confere(
      "o elo · sem MOD",
      registrados(de).length === 1 && registrados(de)[0].modId === null,
      `sem \`modId\`, o \`registrar_da_janela\` não recebeu \`null\`: ${JSON.stringify(invocados.slice(de))}`,
    );
  }
}

// ---------------------------------------------- R2 · o roteador, de verdade

{
  // O recorte começa na tabela de capacidades por versão, e não em
  // `atenderOMod`: a conferência de R2 é feita **antes** do `switch`, e um
  // recorte que a deixasse de fora mediria um roteador que não existe.
  const base = ler("base.js");
  const inicio = base.indexOf("const CAPACIDADES_POR_API = Object.freeze({");
  const roteador = base.indexOf("async function atenderOMod(", inicio);
  const fim = base.indexOf("\nconst modsCarregados =", inicio);
  confere(
    "R2 · o recorte",
    inicio >= 0 && roteador > inicio && fim > roteador,
    "a tabela de capacidades e `atenderOMod` deixaram de ser vizinhos, e o recorte não os achou",
  );

  const registro = new R();
  contexto.contribuicoesDosMods = registro;
  vm.runInContext(base.slice(inicio, fim), contexto);
  // **`motivoDaFalha` mora em `mods-regiao.js`**, que a página carrega antes de
  // `base.js`: o `catch` de `atenderOMod` escreve com ela o motivo no
  // registro, e o R4e, mais abaixo, a recusa do avatar.
  const regiaoFonte = ler("mods-regiao.js");
  const inicioDoMotivo = regiaoFonte.indexOf("function motivoDaFalha(");
  const fimDoMotivo = regiaoFonte.indexOf("\n}\n", inicioDoMotivo) + 3;
  confere("R2 · o recorte do motivo", inicioDoMotivo >= 0,
    "`motivoDaFalha` mudou de forma e o recorte não a achou");
  vm.runInContext(regiaoFonte.slice(inicioDoMotivo, fimDoMotivo), contexto);

  const respostas = [];
  const escuta = { entregar: (m) => respostas.push(JSON.parse(JSON.stringify(m))) };
  const a = new I("mod/a", "a", 7, escuta);
  // A instância real nasce em `criando`; o roteador só atende a `ativa`, e é
  // ela que estamos exercitando.
  a.estado = contexto.ESTADOS_DE_MOD.ativa;
  const b = {
    geracao: 7,
    admite: () => true,
    registrar() { return () => {}; },
    executor: escuta,
  };
  contexto.modsCarregados.set("mod/b", b);
  contexto.modsCarregados.set("mod/a", a);
  const { handle } = registro.registrar({ id: "mod/a" }, a, {
    ponto: "servidor.navegacao", rotulo: "de A",
  });

  (async () => {
    // **B não revoga o que A registrou.** Os punhos são sequenciais: adivinhar
    // o do vizinho não exige nada.
    await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, {
      tipo: "revogar-contribuicao", n: 1, handle,
    });
    confere("R2 · o dono", registro.porHandle.has(handle), "mod/b removeu a contribuição de mod/a");
    confere("R2 · o dono", respostas.at(-1)?.ok === false, "o roteador respondeu sucesso a uma revogação de outro MOD");

    // **As recusas do roteador chegam ao registro com o id do MOD em campo
    // próprio.** Elas sempre foram ditas ao MOD; o registro é de quem hospeda,
    // e o `modId` vai como `mod_id=`, o mesmo campo das linhas do Rust. Quatro
    // portas: a recusa por versão e a mensagem que a API não conhece, que
    // saem antes do `switch` ou no `default` dele, e o `catch`, pela
    // contribuição recusada e pela região com nós demais.
    //
    // A troca começa **antes** da chamada de API 3: começando depois, como
    // começava, a recusa por versão podia sumir do `seele.log` sem que nada
    // aqui reprovasse. Se o roteador lançar, o `catch` do fim encerra a
    // bancada, e a troca não precisa voltar.
    const anotadasDoRoteador = [];
    const registrarDeAntes = contexto.registrarNoAnfitriao;
    contexto.registrarNoAnfitriao = (...argumentos) => anotadasDoRoteador.push(argumentos);
    const dita = (...pedacos) => anotadasDoRoteador.some(([onde, texto, nivel, modId]) =>
      onde === "atender-mod" && nivel === "aviso" && modId === "mod/b"
      && pedacos.every((pedaco) => String(texto).includes(pedaco)));

    // **E um pacote de API 5 não alcança nada: o corte da API 6** (tarefa 13
    // do plano da F3). O manifesto dele já é recusado ao carregar; esta é a
    // segunda porta, a do anfitrião, porque `seele.postar` emite a mensagem do
    // mesmo jeito.
    await contexto.atenderOMod({ id: "mod/b", api: 5 }, b, {
      tipo: "contribuir", n: 2,
      pedido: { ponto: "servidor.navegacao", rotulo: "API 5" },
    });
    confere("R2 · a versão", respostas.at(-1)?.ok === false, "um pacote de API 5 contribuiu pela mensagem direta");
    confere(
      "R2 · a versão",
      String(respostas.at(-1)?.erro ?? "").includes("API 5"),
      `a recusa não disse que a API do pacote é a razão: ${respostas.at(-1)?.erro}`,
    );
    confere(
      "R2 · a versão no registro",
      dita("«contribuir»", "não existe na API 5"),
      "a recusa por versão foi dita só ao MOD, e quem hospeda não acha no seele.log por que o "
        + `pedido dele voltou recusado: ${JSON.stringify(anotadasDoRoteador)}`,
    );

    // E o mesmo pacote declarando API 6 é aceito: a conferência é por versão,
    // e não uma porta fechada.
    await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, {
      tipo: "contribuir", n: 3,
      pedido: { ponto: "servidor.navegacao", rotulo: "API 6" },
    });
    confere("R2 · a versão", respostas.at(-1)?.ok === true, `a API 6 foi recusada: ${respostas.at(-1)?.erro}`);

    // **`atualizar`, pelo roteador** (R7 do PERFIS; tarefa 5, Step 6): B não
    // troca o conteúdo do que é de A, e A troca o dele — com o dono vindo do
    // roteador, e não do corpo da mensagem, como no `revogar`.
    await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, {
      tipo: "atualizar-contribuicao", n: 31, handle, conteudo: [{ forma: "texto", dentro: "de B" }],
    });
    confere("R2 · atualizar, o dono", respostas.at(-1)?.ok === false, "mod/b atualizou a contribuição de mod/a");
    await contexto.atenderOMod({ id: "mod/a", api: 6 }, a, {
      tipo: "atualizar-contribuicao", n: 32, handle, conteudo: [{ forma: "texto", dentro: "de A, de novo" }],
    });
    confere(
      "R2 · atualizar",
      respostas.at(-1)?.ok === true && respostas.at(-1)?.valor === true,
      `mod/a não atualizou a própria contribuição: ${respostas.at(-1)?.erro}`,
    );
    confere(
      "R2 · atualizar",
      registro.porHandle.get(handle)?.conteudo?.[0]?.dentro === "de A, de novo",
      "o roteador respondeu, e o conteúdo da contribuição não mudou",
    );
    // **Atualizar o que já saiu diz que não havia o que trocar** (o PERFIS
    // registra de novo quando o `atualizar` devolve `false`; o cruzamento dos
    // portes): sem recusa, com `false`, e sem a contribuição voltar.
    {
      const { handle: saiu } = registro.registrar({ id: "mod/a" }, a, { ponto: "servidor.navegacao", rotulo: "vai sair" });
      await contexto.atenderOMod({ id: "mod/a", api: 6 }, a, { tipo: "revogar-contribuicao", n: 33, handle: saiu });
      await contexto.atenderOMod({ id: "mod/a", api: 6 }, a, {
        tipo: "atualizar-contribuicao", n: 34, handle: saiu, conteudo: { rotulo: "voltou?" },
      });
      confere(
        "R2 · atualizar o que saiu",
        respostas.at(-1)?.ok === true && respostas.at(-1)?.valor === false && !registro.porHandle.has(saiu),
        `atualizar uma contribuição revogada respondeu ${JSON.stringify(respostas.at(-1))}`,
      );
    }

    // **A recusa chega ao MOD da 6 com o nome** (Step 7b; rascunho §7.1). O
    // teto agregado, pelo roteador de verdade: 128 entradas, e a 129ª. A API 4,
    // que recebia a frase, não roda mais (o corte): ela nem chega ao teto.
    {
      const c = { geracao: 7, admite: () => true, registrar() { return () => {}; }, executor: escuta };
      contexto.modsCarregados.set("mod/c", c);
      for (let i = 0; i < 128; i += 1) registro.registrar({ id: "mod/c" }, c, { ponto: "servidor.navegacao", rotulo: `c${i}` });
      await contexto.atenderOMod({ id: "mod/c", api: 6 }, c, {
        tipo: "contribuir", n: 41, pedido: { ponto: "servidor.navegacao", rotulo: "a 129ª" },
      });
      confere("Step 7b · o nome chega à 6", respostas.at(-1)?.erro === "teto-de-contribuicoes",
        `o MOD da 6 recebeu ${JSON.stringify(respostas.at(-1))}`);
      await contexto.atenderOMod({ id: "mod/c", api: 4 }, c, {
        tipo: "contribuir", n: 42, pedido: { ponto: "servidor.navegacao", rotulo: "a 129ª" },
      });
      confere("Step 7b · a 4 não chega ao teto: é recusada pela versão",
        respostas.at(-1)?.ok === false && String(respostas.at(-1)?.erro ?? "").includes("não existe na API 4"),
        `o MOD da API 4 passou da conferência de versão: ${JSON.stringify(respostas.at(-1))}`);
      contexto.modsCarregados.delete("mod/c");
    }

    // **Um tipo de superfície desconhecido é recusado pelo nome na 6** (tarefa
    // 7, Step 1; rascunho §3 e §7.1). Ele virava `dialogo` em silêncio: o MOD
    // que escreveu `janela` via um diálogo e não sabia por quê, e o produto
    // sabia e não contava. Na 6, `tipo-desconhecido`, com o tipo pedido e os
    // que existem, e a linha no `seele.log`; até a 5 — o que os publicados
    // recebem —, o diálogo de antes, e a linha também. Sem tipo, `dialogo`.
    //
    // Pelo roteador de verdade vai a recusa, com o conjunto de verdade
    // (`SuperficiesDoMod`) e o dono nas duas peças que ela usa: a `api` do
    // pacote e `anotarRecusa`. «O elo» confere as duas no dono de verdade — a
    // `api` e a linha no `seele.log` —, e o roteador responde pela
    // `recusaParaOMod` de `base.js`, no recorte. A recusa sai antes de montar; os
    // casos que montam — sem tipo, e a API 4 — precisam de um DOM que o desta
    // bancada não tem, e conferem a decisão em `tipoDaSuperficie`, que o
    // `criar` usa.
    {
      const anotadasDoDono = [];
      const donoDe = (mod, instancia) => ({
        ...contexto.donoDaRegiao(mod, instancia),
        api: Number(mod.api) || 0,
        anotarRecusa: (oQue) => anotadasDoDono.push(`${mod.id}: ${oQue}`),
      });
      const conjunto = new contexto.SS("mod/b", donoDe({ id: "mod/b", api: 6 }, b), { camadas });
      const superficiesDeAntes = contexto.superficiesDoMod;
      contexto.superficiesDoMod = () => conjunto;

      await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, {
        tipo: "superficie-criar", n: 51, descricao: { id: "x", tipo: "janela" },
      });
      const recusa = respostas.at(-1);
      // **A resposta continua sendo uma resposta.** O campo do tipo pedido
      // chamava-se `tipo`, e `responder` espalha a recusa sobre `{ tipo:
      // "resposta", n, ok }`: a mensagem saía com `tipo: "janela"`, o executor
      // não a reconhecia, e o pedido do MOD esperava para sempre. Este caso
      // passava, porque só olhava o campo (medido na tarefa 7, Step 1).
      confere("Step 1 · a resposta é resposta", recusa?.tipo === "resposta" && recusa?.n === 51,
        `a recusa trocou o tipo ou o número da mensagem da ponte: ${JSON.stringify(recusa)}`);
      confere(
        "Step 1 · o tipo desconhecido, na 6",
        recusa?.ok === false && recusa?.erro === "tipo-desconhecido" && recusa?.tipoPedido === "janela"
          && Array.isArray(recusa?.tipos) && recusa.tipos.includes("dialogo") && !recusa.tipos.includes("janela"),
        `um tipo desconhecido não chegou ao MOD da 6 como \`tipo-desconhecido\`, com \`tipoPedido\` e \`tipos\` — virou dialogo em silêncio, ou perdeu os campos: ${JSON.stringify(recusa)}`,
      );
      confere("Step 1 · o tipo desconhecido, na 6", conjunto.de("x") === null,
        "a superfície recusada ficou de pé");
      confere("Step 1 · o tipo desconhecido no registro", dita("«superficie-criar»", "janela"),
        `a recusa do tipo foi dita só ao MOD: ${JSON.stringify(anotadasDoRoteador.slice(-2))}`);

      // **E nenhuma carga troca o envelope** (`tipo`, `n` e `ok` são da ponte):
      // uma recusa cujos campos tragam os três chega com os da ponte.
      contexto.superficiesDoMod = () => ({
        criar() {
          const falha = new Error("x");
          falha.recusa = "tipo-desconhecido";
          falha.camposDaRecusa = { tipo: "janela", n: 999, ok: true };
          throw falha;
        },
      });
      await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, {
        tipo: "superficie-criar", n: 54, descricao: { id: "y" },
      });
      confere(
        "Step 1 · o envelope é da ponte",
        respostas.at(-1)?.tipo === "resposta" && respostas.at(-1)?.n === 54 && respostas.at(-1)?.ok === false,
        `uma carga trocou o tipo, o número ou o ok da resposta: ${JSON.stringify(respostas.at(-1))}`,
      );
      contexto.superficiesDoMod = superficiesDeAntes;

      // A decisão, caso a caso.
      const decide = (api, descricao) => {
        try {
          return contexto.tipoDaSuperficie(descricao, donoDe({ id: "mod/b", api }, b));
        } catch (falha) {
          return { recusa: falha?.recusa ?? String(falha) };
        }
      };
      confere("Step 1 · sem tipo, dialogo", decide(6, { id: "s" }) === "dialogo"
        && decide(6, { id: "s", tipo: null }) === "dialogo", "uma descrição sem tipo deixou de abrir um dialogo");
      confere("Step 1 · um tipo que existe", decide(6, { id: "p", tipo: "pagina" }) === "pagina",
        "um tipo que existe deixou de ser o pedido");
      confere("Step 1 · o tipo desconhecido", decide(6, { id: "x", tipo: "janela" })?.recusa === "tipo-desconhecido",
        "`tipoDaSuperficie` não recusou o tipo desconhecido");
      // O recuo da 5 — abrir um tipo desconhecido como `dialogo` e dizê-lo ao
      // seele.log — saiu com o corte da API 6 (tarefa 13): só a 6 chega aqui.
    }

    // **E A revoga o que é de A.** Sem esta metade, a correção seria uma porta
    // fechada para todo mundo — e uma porta fechada passa no teste de
    // isolamento sem oferecer a capacidade.
    await contexto.atenderOMod({ id: "mod/a", api: 6 }, a, {
      tipo: "revogar-contribuicao", n: 4, handle,
    });
    confere("R2 · o dono", respostas.at(-1)?.ok === true, `mod/a não conseguiu revogar a própria contribuição: ${respostas.at(-1)?.erro}`);
    confere("R2 · o dono", respostas.at(-1)?.valor === true, "a revogação do dono disse que não havia o que tirar");
    confere("R2 · o dono", registro.porHandle.has(handle) === false, "a revogação do dono não tirou a contribuição");
    confere("R2 · o dono", a.recursos.length === 0, "a revogação do dono deixou o descartador retido na instância");

    // **O `catch` de `atenderOMod`**: a contribuição recusada, e a mensagem
    // `regiao`, que a 6 não tem. (A região com nós demais, pelo
    // `desenharARegiaoDoMod` de verdade, saiu com a API 3.)
    {
      let contribuicaoRespondida;
      let regiaoRespondida;
      {
        await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, {
          tipo: "contribuir", n: 5,
          pedido: { ponto: "nao.existe", rotulo: "x" },
        });
        contribuicaoRespondida = respostas.at(-1);
        await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, {
          tipo: "regiao", n: 6,
          conteudo: [{ forma: "texto", chave: "t", dentro: "x" }],
        });
        regiaoRespondida = respostas.at(-1);
      }
      confere("R2 · a recusa no registro", contribuicaoRespondida?.ok === false
        && contribuicaoRespondida.erro === "ponto-desconhecido",
        `um ponto que não existe foi aceito, ou recusado sem o nome: ${JSON.stringify(contribuicaoRespondida)}`);
      // **E a lista do que existe vai ao MOD**, no molde do `tipo-desconhecido`
      // (`contribuicoes.fora[1]`; a orquestradora, 16/10): o ponto pedido em
      // `pontoPedido`, e os que a API tem em `pontos`. Antes ela ia só ao
      // registro.
      confere("R2 · a lista ao MOD", contribuicaoRespondida?.pontoPedido === "nao.existe"
        && Array.isArray(contribuicaoRespondida.pontos)
        && contribuicaoRespondida.pontos.includes("pessoa.cartao")
        && contribuicaoRespondida.pontos.includes("canal.item")
        && !contribuicaoRespondida.pontos.includes("nao.existe"),
        `o MOD não recebeu o ponto pedido e os que existem: ${JSON.stringify(contribuicaoRespondida)}`);
      confere(
        "R2 · a recusa no registro",
        dita("nao.existe"),
        `a contribuição recusada não chegou ao registro com o nível e o id: ${JSON.stringify(anotadasDoRoteador)}`,
      );
      // **O motivo, e não a frase de tela.** `fraseDeErro` é a fronteira com
      // quem usa, e para um `Error` da janela ela começa por «ALGO FALHOU E
      // ESTE APP NÃO SABE EXPLICAR O QUÊ»: na linha de quem hospeda, isso punha
      // um pedido de desculpas na frente do motivo.
      const linhaDaRecusada = anotadasDoRoteador.find(([, texto]) => String(texto).includes("nao.existe"));
      confere(
        "R2 · a recusa no registro",
        linhaDaRecusada !== undefined && !String(linhaDaRecusada[1]).includes("ALGO FALHOU"),
        "a linha da contribuição recusada levou ao seele.log a frase de tela de `fraseDeErro`, "
          + `e não só o motivo: ${JSON.stringify(linhaDaRecusada)}`,
      );
      // **Com a lista do que existe** (`contribuicoes.fora[1]`): a linha diz
      // os pontos que a API tem, e não só o que veio.
      confere(
        "R2 · a recusa no registro",
        /os que existem são .*pessoa\.cartao.*canal\.item/.test(String(linhaDaRecusada?.[1] ?? "")),
        `a linha da contribuição recusada não disse os pontos que existem: ${JSON.stringify(linhaDaRecusada)}`,
      );
      // **A 6 não tem `regiao`** (o corte; rascunho §2): a mensagem é recusada
      // pela versão antes do renderer, e a recusa chega ao registro. A região
      // com nós demais, na 6, é a contribuição num ponto de região, coberta
      // pela recusa acima.
      confere("R2 · a região saiu da 6", regiaoRespondida?.ok === false,
        "um pacote da API 6 desenhou pela mensagem `regiao`");
      confere(
        "R2 · a região saiu da 6, no registro",
        // Sem `case` desde a limpeza da máquina das APIs 3 a 5: a mensagem cai
        // no `default`, que diz que a API não a conhece.
        dita("«regiao»", "não conhece"),
        `a recusa da região da 6 não chegou ao registro com o nível e o id: ${JSON.stringify(anotadasDoRoteador)}`,
      );
    }

    // **A mensagem que a API não conhece**, no `default` do `switch`: a
    // conferência de versão deixa passar o tipo que não está em
    // `CAPACIDADE_DA_MENSAGEM`, e é aqui que ele é recusado. Pelo tipo, e não
    // só por «não conhece»: a recusa do ponto «nao.existe», acima, também diz
    // isso.
    await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, { tipo: "inexistente", n: 7 });
    confere("R2 · o tipo desconhecido", respostas.at(-1)?.ok === false,
      "uma mensagem que a API não conhece foi respondida como aceita");
    confere(
      "R2 · o tipo desconhecido no registro",
      dita("não conhece «inexistente»"),
      "a mensagem que a API não conhece foi recusada só para o MOD, e o seele.log não diz que "
        + `ela chegou: ${JSON.stringify(anotadasDoRoteador)}`,
    );

    // **Uma variante de struct da FFI**, que `motivoDaFalha` não sabe ler: ela
    // a escreveria «[object Object]», e o registro a leva inteira em JSON. O
    // `retrato` é o caso por onde ela entraria na 6 — o `invoke` do snapshot
    // que rejeita —, e `Refused` é uma variante assim do `ConnectionError`.
    contexto.invoke = () => Promise.reject({ Refused: { reason: "Teste" } });
    try {
      await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, { tipo: "retrato", n: 8 });
    } finally {
      delete contexto.invoke;
    }
    confere("R2 · a variante da FFI", respostas.at(-1)?.ok === false,
      "um retrato cujo `invoke` rejeitou foi respondido como aceito");
    confere(
      "R2 · a variante da FFI",
      dita("«retrato»", '{"Refused":{"reason":"Teste"}}')
        && !anotadasDoRoteador.some(([, texto]) => String(texto).includes("[object Object]")),
      "a falha que chega como variante de struct da FFI não foi ao registro inteira: "
        + `${JSON.stringify(anotadasDoRoteador)}`,
    );
    // **A recusa chega ao MOD da API 6 pelo nome, e não pela frase** (o K1-m3
    // da F5; rascunho da API 6, §7.1). A recusa nomeada do Rust vira
    // `{ erro: nome, …campos }` — `tipo-recusado:heic` vira `erro:
    // 'tipo-recusado'` e `detectado: 'heic'` —, uma falha sem nome vira
    // `falhou`, e o motivo vai ao registro. Até a 5, a frase de antes.
    const recusarComo = (falha) => {
      contexto.invoke = () => Promise.reject(falha);
    };
    try {
      recusarComo({ Recusado: { motivo: "tipo-recusado:heic" } });
      await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, { tipo: "enviar-imagem", n: 30, arquivo: 1, token: "t" });
      const seis = respostas.at(-1);
      confere("recusa · a 6 pelo nome", seis?.ok === false && seis?.erro === "tipo-recusado"
        && seis?.detectado === "heic",
      `o MOD da API 6 não recebeu a recusa pelo nome: ${JSON.stringify(seis)}`);

      recusarComo({ Recusado: { motivo: "sessao-encerrada" } });
      await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, { tipo: "enviar-imagem", n: 31, arquivo: 1, token: "t" });
      confere("recusa · o nome sem sufixo", respostas.at(-1)?.erro === "sessao-encerrada"
        && !("detectado" in respostas.at(-1)),
      `um nome sem sufixo não chegou como veio: ${JSON.stringify(respostas.at(-1))}`);

      recusarComo(new Error("o disco sumiu"));
      await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, { tipo: "enviar-imagem", n: 32, arquivo: 1, token: "t" });
      confere("recusa · a falha sem nome", respostas.at(-1)?.erro === "falhou",
        `uma falha sem nome não virou «falhou»: ${JSON.stringify(respostas.at(-1))}`);
      confere("recusa · a falha sem nome no registro", dita("«enviar-imagem»", "o disco sumiu"),
        `o motivo da falha sem nome não foi ao registro: ${JSON.stringify(anotadasDoRoteador)}`);

      // **A lista fechada vale na fronteira** (a auditoria do `v6.json`, B2;
      // `recusas.o-que-e`). Um nome que o Rust manda e que a lista não tem —
      // o prazo do enlace era uma frase, que chegava inteira em `e.erro` —
      // chega como `falhou`, sem os campos que vieram com ele, e o que veio
      // vai ao registro.
      const prazo = "Prazo da transferência esgotado. Confira a conexão e a versão do servidor.";
      recusarComo({ Recusado: { motivo: prazo } });
      await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, { tipo: "enviar-imagem", n: 34, arquivo: 1, token: "t" });
      const fora = respostas.at(-1);
      confere("lista fechada · um nome fora dela", fora?.ok === false && fora?.erro === "falhou"
        && Object.keys(fora).every((campo) => ["tipo", "n", "ok", "erro"].includes(campo)),
      `uma recusa fora da lista fechada chegou ao MOD como veio: ${JSON.stringify(fora)}`);
      confere("lista fechada · um nome fora dela, no registro", dita("Prazo da transferência esgotado"),
        `a recusa fora da lista não foi ao registro: ${JSON.stringify(anotadasDoRoteador)}`);
    } finally {
      delete contexto.invoke;
    }

    // **O tipo que o pacote não pode pedir chega pelo nome**: o que a API não
    // conhece e o que saiu dela na 6 são `tipo-desconhecido`, com o
    // `tipoPedido`, como o tipo de superfície que não é da API. Eram frases.
    await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, { tipo: "inexistente", n: 35 });
    const desconhecido = respostas.at(-1);
    confere("lista fechada · o tipo que a API não conhece", desconhecido?.erro === "tipo-desconhecido"
      && desconhecido?.tipoPedido === "inexistente",
    `o tipo que a API não conhece não chegou pelo nome: ${JSON.stringify(desconhecido)}`);
    await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, { tipo: "snapshot", n: 36 });
    const saiu = respostas.at(-1);
    confere("lista fechada · o tipo que saiu da 6", saiu?.erro === "tipo-desconhecido"
      && saiu?.tipoPedido === "snapshot",
    `o tipo que saiu da 6 não chegou pelo nome: ${JSON.stringify(saiu)}`);

    // **E o pedido que não cabe na mensagem é `mensagem-grande`**, como o
    // prelúdio o chama: a janela recusava de novo com `request-too-large`,
    // que não tinha nome para o MOD e chegava como `falhou`.
    // O `pedirAoServidor` fica antes do recorte do roteador: recortado aqui,
    // da fila de pedidos ao fim dele, com o que ele lê de fora em dublê — o
    // mapa dos pedidos, o contador, a escuta e a geração de pé.
    const deFila = base.indexOf("let filaDePedidosDeMod = Promise.resolve();");
    const dePedir = base.indexOf("async function pedirAoServidor(", deFila);
    const fimDePedir = base.indexOf("\n}\n", dePedir) + 3;
    confere("lista fechada · o recorte do pedido", deFila >= 0 && dePedir > deFila,
      "a fila de pedidos e `pedirAoServidor` deixaram de ser vizinhas, e o recorte não as achou");
    vm.runInContext("const pedidosDeMod = new Map(); let proximoPedidoDeMod = 0;", contexto);
    contexto.ouvirMod = Promise.resolve();
    contexto.daGeracaoDePe = () => true;
    vm.runInContext(base.slice(deFila, fimDePedir), contexto);
    // A vez na fila espera 125 ms por um `setTimeout`, que nesta bancada não
    // dispara: o caso mede o nome da recusa de tamanho, e não a fila.
    contexto.aguardarVezDePedidoDeMod = async () => {};
    contexto.TextEncoder = TextEncoder;
    try {
      contexto.invoke = () => Promise.reject(new Error("o pedido grande não podia chegar ao Rust"));
      await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, {
        tipo: "pedido", n: 37, canal: 0, valor: { texto: "x".repeat(13 * 1024) },
      });
      const grande = respostas.at(-1);
      confere("lista fechada · o pedido grande", grande?.erro === "mensagem-grande",
        `o pedido grande demais não chegou como mensagem-grande: ${JSON.stringify(grande)}; `
          + `o registro: ${JSON.stringify(anotadasDoRoteador.at(-1))}`);

      recusarComo({ Recusado: { motivo: "tipo-recusado:heic" } });
      await contexto.atenderOMod({ id: "mod/b", api: 5 }, b, { tipo: "enviar-imagem", n: 33, arquivo: 1, token: "t" });
      const cinco = respostas.at(-1);
      confere("recusa · a 5 é recusada pela versão", cinco?.ok === false
        && String(cinco?.erro ?? "").includes("não existe na API 5"),
      `o MOD da API 5 passou da conferência de versão: ${JSON.stringify(cinco)}`);
    } finally {
      delete contexto.invoke;
    }

    // **O retrato da API 6 no lugar do `snapshot`** (tarefa 4 do plano da F3).
    // A 6 pede `retrato` e recebe o projetado por lista branca; a 5 não
    // alcança o `retrato`; e a 6 que emite `snapshot` direto, por
    // `seele.postar`, é recusada — o prelúdio dela não monta o método, e a
    // fronteira é esta conferência.
    const snapshotDoRoteador = {
      me: 3, server: "Casa", link_state: "ConexaoSegura", caminho: "rede-local",
      telemetry: { microfone_entrega_silencio_digital: true },
      presentes: [{ id: 3, nickname: "ana", speaking: false, muted: false, is_self: true }],
      voice_rooms: [], channels: [{ id: 7, name: "mesa", open: true }],
    };
    contexto.invoke = (comando) => (comando === "snapshot"
      ? Promise.resolve(snapshotDoRoteador)
      : Promise.reject(new Error(`invoke inesperado: ${comando}`)));
    try {
      await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, { tipo: "retrato", n: 20 });
      const retrato = respostas.at(-1);
      confere("retrato · a 6 lê o projetado", retrato?.ok === true
        && retrato.valor?.eu === 3 && retrato.valor?.canais?.[0]?.aberto === true
        && !JSON.stringify(retrato.valor).includes("link_state")
        && !JSON.stringify(retrato.valor).includes("microfone_entrega_silencio_digital"),
      `o retrato da API 6 não veio projetado: ${JSON.stringify(retrato)}`);

      await contexto.atenderOMod({ id: "mod/b", api: 5 }, b, { tipo: "retrato", n: 21 });
      confere("retrato · a 5 não alcança", respostas.at(-1)?.ok === false,
        "um pacote da API 5 leu o retrato da 6");

      await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, { tipo: "snapshot", n: 22 });
      confere("retrato · o snapshot saiu da 6", respostas.at(-1)?.ok === false,
        `um pacote da API 6 leu o Snapshot inteiro pela mensagem direta: ${JSON.stringify(respostas.at(-1))}`);
      confere("retrato · o snapshot saiu da 6, no registro", dita("«snapshot»", "não conhece"),
        `a recusa do snapshot da 6 foi dita só ao MOD: ${JSON.stringify(anotadasDoRoteador)}`);

      // **E os `cartoes` também saíram da 6** (`compatibilidade.saem.cartoes`):
      // o cartão é o `pessoa.cartao` com alvo. A mensagem cai no `default`, com
      // o tipo pedido, e a recusa chega ao registro.
      await contexto.atenderOMod({ id: "mod/b", api: 6 }, b, {
        tipo: "cartoes", n: 24, cartoes: { 3: [{ forma: "texto", dentro: "x" }] },
      });
      const cartoes = respostas.at(-1);
      confere("retrato · os cartoes saíram da 6", cartoes?.ok === false
        && cartoes.erro === "tipo-desconhecido" && cartoes.tipoPedido === "cartoes",
      `um pacote da API 6 deu cartões pela mensagem da API 3: ${JSON.stringify(cartoes)}`);
      confere("retrato · os cartoes saíram da 6, no registro", dita("«cartoes»", "não conhece"),
        `a recusa dos cartões da 6 foi dita só ao MOD: ${JSON.stringify(anotadasDoRoteador)}`);

      // **E a 5 não alcança nem o `snapshot`** (o corte): uma API que este app
      // não roda não tem capacidade nenhuma, e a mensagem que não exige
      // capacidade também é recusada. Antes do corte, o `snapshot` inteiro
      // passava para qualquer API fora da tabela.
      await contexto.atenderOMod({ id: "mod/b", api: 5 }, b, { tipo: "snapshot", n: 23 });
      confere("retrato · a 5 não alcança nem o snapshot", respostas.at(-1)?.ok === false
        && !JSON.stringify(respostas.at(-1)).includes("ConexaoSegura"),
      `um pacote da API 5 leu o Snapshot inteiro: ${JSON.stringify(respostas.at(-1))}`);
    } finally {
      delete contexto.invoke;
    }
    contexto.registrarNoAnfitriao = registrarDeAntes;

    // **R4e · o avatar que não montou é dito.** Aqui, dentro do roteador, e não
    // em R4d: a recusa chega numa promessa, e este é o trecho da bancada que
    // espera antes de `terminar`. É o caso de 23/09 — o avatar do PERFIS.
    //
    // Três chamadas: a que a janela recusa (lança um `Error`), a que o Rust
    // recusa, que chega como `{ Recusado: { motivo } }` e não como `Error`, e a
    // imagem que o navegador não decodifica. Só a segunda separa
    // `motivoDaFalha` de `String(falha?.message ?? falha)` — com esta, o
    // registro voltaria a dizer «[object Object]». A terceira é a pendência
    // 51, item 7: o cabeçalho diz imagem, o corpo não abre, e o avatar só vale
    // depois do `decode()`.
    {
      // `motivoDaFalha` já está no contexto: ela entra com o roteador, no
      // começo do R2. `provarImagem` e o teto do cartão moram no mesmo
      // `mods-regiao.js`, e entram aqui: o avatar chama as duas antes de dar a
      // imagem por pronta.
      // Até o fim de `imagemContribuida`, que o avatar chama desde a tarefa 6
      // (a mesma carga para o avatar e a capa).
      const inicioDoAvatar = base.indexOf("function avatarContribuido(");
      const fimDoAvatar = base.indexOf("\n}\n", base.indexOf("function imagemContribuida(", inicioDoAvatar)) + 3;
      confere("R4e · o recorte", inicioDoAvatar >= 0,
        "`avatarContribuido` mudou de forma e o recorte não a achou");
      vm.runInContext(base.slice(inicioDoAvatar, fimDoAvatar), contexto);
      // E `soImagem`, com o que ela chama (M5 da revisão do K4): o avatar só
      // mostra imagem, e o som que o Rust recusa pelo teto chega a ele como o
      // som que era.
      for (const [de, ate, quem] of [
        ["function provarImagem(", "\n}\n", "`provarImagem`"],
        ["const LIMITES_DO_CARTAO = Object.freeze({", "\n});\n", "`LIMITES_DO_CARTAO`"],
        ["const SOM_GRANDE_DEMAIS_DECODIFICADO = ", ";\n", "`SOM_GRANDE_DEMAIS_DECODIFICADO`"],
        ["function recusaDoSomGrandeDemais(", "\n}\n", "`recusaDoSomGrandeDemais`"],
        ["function soImagem(", "\n}\n", "`soImagem`"],
      ]) {
        const inicio = regiaoFonte.indexOf(de);
        confere("R4e · o recorte da imagem", inicio >= 0,
          `${quem} não está em \`mods-regiao.js\`, e o avatar não tem como provar a imagem antes de mostrá-la`);
        if (inicio >= 0) vm.runInContext(regiaoFonte.slice(inicio, regiaoFonte.indexOf(ate, inicio) + ate.length), contexto);
      }
      // O `Image` da janela, de mentira: decodifica tudo, menos o corpo
      // corrompido — e segura em `presas` a decodificação de quem começa com
      // `URI_QUE_DEMORA`, até a prova soltar.
      const URI_QUE_NAO_DECODIFICA = "data:image/png;base64,iVBORw0KGgoCORPOQUEBRADO";
      const URI_QUE_DEMORA = "data:image/png;base64,iVBORw0KGgoDEMORA";
      const presas = [];
      contexto.Image = class {
        constructor() {
          this.src = "";
        }
        decode() {
          if (this.src.startsWith(URI_QUE_DEMORA)) {
            return new Promise((pronto, falhou) => presas.push({ pronto, falhou }));
          }
          return this.src === URI_QUE_NAO_DECODIFICA
            ? Promise.reject(new Error("EncodingError: The source image cannot be decoded."))
            : Promise.resolve();
        }
      };
      for (const [pessoa, carregar, motivo] of [
        [12, () => Promise.resolve({ uri: "x:", papel: "som", bytes: 2 }), "não é imagem"],
        [13, () => Promise.reject({ Recusado: { motivo: "fora do teto" } }), "fora do teto"],
        [14, () => Promise.resolve({ uri: URI_QUE_NAO_DECODIFICA, papel: "imagem", bytes: 2 }), "não decodificou"],
        // O som que o Rust recusou pelo teto do som: para o avatar, que só
        // mostra imagem, o MOD ouve o «não é imagem» de antes (M5 da revisão
        // do K4), e não a recusa do som.
        [15, () => Promise.reject({
          Recusado: { motivo: "som-grande-demais-decodificado: decodificado a 48000 Hz, o som ocupa ao menos 251657180 bytes, acima do teto de 67108864 bytes de som" },
        }), "não é imagem"],
      ]) {
        const ditas = [];
        const faladas = [];
        const contribuicao = {
          mod: "seele/perfis", instancia: {},
          conteudo: { doServidor: { canal: 1, pedido: {}, campo: "image" } },
        };
        const registroDeAntes = contexto.contribuicoesDosMods;
        const donoDeAntes = contexto.donoDaRegiao;
        contexto.contribuicoesDosMods = {
          escolherSubstituicao: () => ({ escolhida: contribuicao }),
          porHandle: new Map(),
        };
        contexto.donoDaRegiao = () => ({
          podeFalar: () => true,
          falar: (dados) => faladas.push(dados),
          anotarRecusa: (texto) => ditas.push(String(texto)),
          carregarMidiaDoServidor: carregar,
        });
        // **O aviso à gestão mora em `base.js`, e não no recorte.** Desde a fase
        // M1 da casca, o retrato diz em que pé a carga está e avisa quem desenha
        // «quem pinta cada lugar»; aqui o aviso só é contado.
        let avisosDeMidia = 0;
        const avisarDeAntes = contexto.avisarQueAMidiaMudou;
        contexto.avisarQueAMidiaMudou = () => {
          avisosDeMidia += 1;
        };
        // O redesenho também mora em `base.js`: uma imagem dada por pronta o
        // chama, e aqui ele só existe para não lançar no lugar da recusa.
        contexto.redesenharAvatares = () => {};
        try {
          confere("R4e · avatar", contexto.avatarContribuido(pessoa) === null,
            "o avatar devolveu uma imagem antes de ela chegar");
          await new Promise((resolve) => setImmediate(resolve));
        } finally {
          contexto.contribuicoesDosMods = registroDeAntes;
          contexto.donoDaRegiao = donoDeAntes;
          contexto.avisarQueAMidiaMudou = avisarDeAntes;
          delete contexto.redesenharAvatares;
        }
        // Com o motivo **do caso**, e não com um só: a recusa da janela diz «não
        // é imagem», e a do Rust diz o motivo que ele deu.
        confere(
          "R4e · a gestão",
          contribuicao.retrato?.situacao === "recusada"
            && String(contribuicao.retrato?.motivo).includes(motivo)
            && avisosDeMidia >= 2,
          `o retrato recusado (${motivo}) não disse em que pé ficou, ou não avisou a gestão: `
          + `${JSON.stringify(contribuicao.retrato)} (${avisosDeMidia} aviso(s))`,
        );
        confere(
          "R4e · avatar",
          ditas.some((texto) => texto.includes(`avatar da pessoa ${pessoa}`) && texto.includes(motivo)),
          `o avatar que não montou (${motivo}) não chegou ao registro com o motivo: ${JSON.stringify(ditas)}`,
        );
        confere(
          "R4e · avatar",
          !ditas.some((texto) => texto.includes("[object Object]")),
          `a recusa do Rust no avatar virou «[object Object]» no registro: ${JSON.stringify(ditas)}`,
        );
        confere(
          "R4e · avatar",
          faladas.some((dito) => dito.nome === "midia" && dito.chave === "avatar" && dito.estado === "falhou"
            && String(dito.porque).includes(motivo)),
          `o MOD não recebeu «midia»/«falhou» do avatar que não montou (${motivo}): ${JSON.stringify(faladas)}`,
        );
        // **A reserva volta.** Os bytes do avatar contam no teto do cartão
        // **antes** do `decode()`, para duas imagens decodificando juntas não
        // passarem as duas por ele; a imagem que não abre os devolve.
        confere(
          "R4e · avatar",
          contribuicao.retrato?.bytes === 0 && contribuicao.retrato?.uri === null,
          `o avatar recusado (${motivo}) continua ocupando o teto do cartão, ou ficou com uma imagem: `
            + `${JSON.stringify(contribuicao.retrato)}`,
        );
      }

      // **E a imagem que abre**: pronta, contada e redesenhada — depois do
      // `decode()`, e não antes.
      {
        const uri = "data:image/png;base64,AA";
        const contribuicao = {
          mod: "seele/perfis", instancia: {},
          conteudo: { doServidor: { canal: 1, pedido: {}, campo: "image" } },
        };
        const registroDeAntes = contexto.contribuicoesDosMods;
        const donoDeAntes = contexto.donoDaRegiao;
        const avisarDeAntes = contexto.avisarQueAMidiaMudou;
        let redesenhos = 0;
        contexto.contribuicoesDosMods = {
          escolherSubstituicao: () => ({ escolhida: contribuicao }),
          porHandle: new Map(),
        };
        contexto.donoDaRegiao = () => ({
          podeFalar: () => true,
          falar() {},
          anotarRecusa() {},
          carregarMidiaDoServidor: () => Promise.resolve({ uri, papel: "imagem", bytes: 2 }),
        });
        contexto.avisarQueAMidiaMudou = () => {};
        contexto.redesenharAvatares = () => {
          redesenhos += 1;
        };
        try {
          confere("R4e · o avatar que abre", contexto.avatarContribuido(15) === null,
            "o avatar devolveu a imagem antes de ela chegar");
          await new Promise((resolve) => setImmediate(resolve));
          confere(
            "R4e · o avatar que abre",
            contribuicao.retrato?.situacao === "pronta" && contribuicao.retrato?.bytes === 2 && redesenhos === 1
              && contexto.avatarContribuido(15) === uri,
            `a imagem que decodifica não virou o avatar: ${JSON.stringify(contribuicao.retrato)}, `
              + `${redesenhos} redesenho(s)`,
          );
        } finally {
          contexto.contribuicoesDosMods = registroDeAntes;
          contexto.donoDaRegiao = donoDeAntes;
          contexto.avisarQueAMidiaMudou = avisarDeAntes;
          delete contexto.redesenharAvatares;
        }
      }

      // **Um redesenho que lança não é uma recusa** (rodada 1 de conserto do
      // lote K2). A imagem decodificou e virou o avatar; se o redesenho lança
      // depois, o `catch` da recusa a dizia «recusada», zerava os bytes e
      // deixava o `uri` posto — o avatar visível, recusado e fora da conta. O
      // estado fica o que a tela mostra, e o tropeço é dito no `console` e no
      // `seele.log` de quem opera.
      //
      // **O `anotarRecusa` aqui é o de verdade**, o de `base.js`, até um
      // `invoke` de mentira, num contexto só dele (como «o elo», acima): é ele
      // quem põe o `onde` das linhas de mídia e o id do MOD em campo próprio, e
      // um anotador de mentira não mediria nenhum dos dois.
      {
        const uri = "data:image/png;base64,AB";
        const contribuicao = {
          mod: "seele/perfis", instancia: {},
          conteudo: { doServidor: { canal: 1, pedido: {}, campo: "image" } },
        };
        const registroDeAntes = contexto.contribuicoesDosMods;
        const donoDeAntes = contexto.donoDaRegiao;
        const avisarDeAntes = contexto.avisarQueAMidiaMudou;
        const consoleDeAntes = contexto.console;
        const faladas = [];
        const linhas = [];
        const avisos = [];
        const soltas = [];
        const pegar = (motivo) => soltas.push(String(motivo?.stack ?? motivo));
        const doRegistro = vm.createContext({
          console,
          geracaoDaSessao: 7,
          modsCarregados: new Map(),
          invoke: (cmd, args) => {
            if (cmd === "registrar_da_janela") linhas.push(args);
            return Promise.resolve();
          },
          avisarQueAMidiaMudou: () => {},
        });
        for (const de of ["function registrarNoAnfitriao(", "function donoDaRegiao("]) {
          const inicio = base.indexOf(de);
          const fim = base.indexOf("\n}\n", inicio);
          confere("R4e · o recorte do registro", inicio >= 0 && fim > inicio,
            `«${de}» mudou de forma, e o recorte até o primeiro «\\n}\\n» não a achou`);
          if (inicio >= 0 && fim > inicio) vm.runInContext(base.slice(inicio, fim + 3), doRegistro);
        }
        const instanciaDoRegistro = { geracao: 7, admite: () => true };
        doRegistro.modsCarregados.set("seele/perfis", instanciaDoRegistro);
        const anotarRecusa = doRegistro.donoDaRegiao({ id: "seele/perfis", hash: "h" }, instanciaDoRegistro).anotarRecusa;
        contexto.contribuicoesDosMods = {
          escolherSubstituicao: () => ({ escolhida: contribuicao }),
          porHandle: new Map(),
        };
        contexto.donoDaRegiao = () => ({
          podeFalar: () => true,
          falar: (dados) => faladas.push(dados),
          anotarRecusa,
          carregarMidiaDoServidor: () => Promise.resolve({ uri, papel: "imagem", bytes: 2 }),
        });
        contexto.avisarQueAMidiaMudou = () => {};
        contexto.redesenharAvatares = () => {
          throw new Error("o redesenho caiu");
        };
        contexto.console = { ...console, warn: (...partes) => avisos.push(partes.map((p) => String(p?.message ?? p)).join(" ")) };
        process.on("unhandledRejection", pegar);
        try {
          contexto.avatarContribuido(17);
          await new Promise((resolve) => setImmediate(resolve));
          await new Promise((resolve) => setImmediate(resolve));
          confere(
            "R4e · o redesenho que lança",
            contribuicao.retrato?.situacao === "pronta" && contribuicao.retrato?.bytes === 2
              && contribuicao.retrato?.uri === uri,
            "o avatar decodificou e o redesenho lançou, e o avatar ficou recusado, fora da conta ou sem a imagem: "
              + `${JSON.stringify(contribuicao.retrato)}`,
          );
          confere(
            "R4e · o redesenho que lança",
            faladas.length === 0,
            `o tropeço do redesenho foi dito ao MOD como recusa: ${JSON.stringify(faladas)}`,
          );
          const linha = linhas[0];
          confere(
            "R4e · o redesenho que lança",
            linhas.length === 1
              && linha.nivel === "aviso" && linha.onde === "recusa-de-mod" && linha.modId === "seele/perfis"
              && String(linha.oQue).startsWith("seele/perfis: avatar da pessoa 17")
              && String(linha.oQue).includes("tropeçou") && String(linha.oQue).includes("o redesenho caiu")
              && !String(linha.oQue).includes("não montou"),
            "o tropeço do redesenho não chegou ao seele.log como uma linha só, no `onde` das linhas de mídia e com o id "
              + `do MOD em campo próprio — ou chegou dito como recusa: ${JSON.stringify(linhas)}`,
          );
          confere(
            "R4e · o redesenho que lança",
            avisos.some((aviso) => aviso.includes("o redesenho caiu")) && soltas.length === 0,
            `o tropeço do redesenho não foi dito no console, ou terminou numa promessa que ninguém pegou: ${JSON.stringify(avisos)} `
              + `${soltas.join(" | ")}`,
          );
        } finally {
          process.off("unhandledRejection", pegar);
          contexto.contribuicoesDosMods = registroDeAntes;
          contexto.donoDaRegiao = donoDeAntes;
          contexto.avisarQueAMidiaMudou = avisarDeAntes;
          contexto.console = consoleDeAntes;
          delete contexto.redesenharAvatares;
        }
      }

      // **A sessão acaba no meio do `decode()`.** O avatar reservou os bytes
      // antes de decodificar; quando a decodificação termina, `podeFalar()` já
      // diz que a instância não é a de agora. A imagem não vira o avatar, e a
      // reserva volta.
      {
        const contribuicao = {
          mod: "seele/perfis", instancia: {},
          conteudo: { doServidor: { canal: 1, pedido: {}, campo: "image" } },
        };
        const registroDeAntes = contexto.contribuicoesDosMods;
        const donoDeAntes = contexto.donoDaRegiao;
        const avisarDeAntes = contexto.avisarQueAMidiaMudou;
        let falando = true;
        let redesenhos = 0;
        contexto.contribuicoesDosMods = {
          escolherSubstituicao: () => ({ escolhida: contribuicao }),
          porHandle: new Map(),
        };
        contexto.donoDaRegiao = () => ({
          podeFalar: () => falando,
          falar() {},
          anotarRecusa() {},
          carregarMidiaDoServidor: () => Promise.resolve({ uri: `${URI_QUE_DEMORA}-sessao`, papel: "imagem", bytes: 2 }),
        });
        contexto.avisarQueAMidiaMudou = () => {};
        contexto.redesenharAvatares = () => {
          redesenhos += 1;
        };
        try {
          contexto.avatarContribuido(16);
          await new Promise((resolve) => setImmediate(resolve));
          confere(
            "R4e · a sessão acaba no meio",
            presas.length === 1 && contribuicao.retrato?.bytes === 2,
            `o avatar não ficou reservado enquanto decodificava: ${JSON.stringify(contribuicao.retrato)}`,
          );
          falando = false;
          presas.shift()?.pronto();
          await new Promise((resolve) => setImmediate(resolve));
          confere(
            "R4e · a sessão acaba no meio",
            contribuicao.retrato?.bytes === 0 && contribuicao.retrato?.uri === null
              && contribuicao.retrato?.situacao !== "pronta" && redesenhos === 0,
            "o avatar decodificado depois de a sessão acabar virou a imagem da pessoa, ou ficou ocupando o teto "
              + `do cartão: ${JSON.stringify(contribuicao.retrato)}, ${redesenhos} redesenho(s)`,
          );
        } finally {
          contexto.contribuicoesDosMods = registroDeAntes;
          contexto.donoDaRegiao = donoDeAntes;
          contexto.avisarQueAMidiaMudou = avisarDeAntes;
          delete contexto.redesenharAvatares;
        }
      }
    }


    // ------------- R4e · a saída da instância leva as superfícies, sem pergunta
    //
    // `superficies.descarte` (a auditoria do v6, tabela 7): «sai com a
    // instância; a saída da sessão descarta sem perguntar». R4c descarta à mão;
    // aqui quem descarta é o `encerrar` da instância, com um diálogo **sujo**
    // aberto — que, fechado pela pessoa, perguntaria —, por cada um dos dois
    // registros: o do conjunto (`superficiesDoMod`, de base.js, de verdade) e
    // o de cada superfície. Tirar qualquer um passava.
    {
      const base = ler("base.js");
      const inicio = base.indexOf("function superficiesDoMod(");
      const fim = base.indexOf("\n}\n", inicio) + 3;
      confere("R4e · o recorte", inicio >= 0 && base.slice(inicio, fim).includes("instancia.registrar("),
        "`superficiesDoMod` mudou de forma e o recorte não a achou");
      contexto.superficiesDosMods = new Map();
      contexto.palcosDeSuperficie = () => ({});
      vm.runInContext(base.slice(inicio, fim), contexto);
      const cascaReal = S.prototype.montarCasca;
      S.prototype.montarCasca = function montarCascaDeTeste() {
        this.raiz = new No("div", "");
        this.corpo = new No("div", "");
        // O diálogo sai pela camada dele, e não pela raiz.
        this.camada = new No("div", "");
      };
      const perguntas = [];
      contexto.abrirConfirmacao = (titulo) => perguntas.push(titulo);
      const deSaida = () => {
        const instancia = new I("mod/s", "s", 7, { pedirEncerramento() {}, async encerrou() { return true; } });
        instancia.estado = contexto.ESTADOS_DE_MOD.ativa;
        return instancia;
      };
      try {
        // Pelo conjunto, como o roteador cria. A superfície entra nele sem se
        // registrar na instância: só o registro do conjunto a tira.
        const doConjunto = deSaida();
        const conjunto = contexto.superficiesDoMod({ id: "mod/s" }, doConjunto);
        const noConjunto = new S("mod/s", { instancia: null, falar() {} }, { id: "d", tipo: "dialogo" }, {});
        noConjunto.marcarSuja(true);
        conjunto.porChave.set("d", noConjunto);
        await doConjunto.encerrar();
        confere("R4e", noConjunto.solta && conjunto.porChave.size === 0 && !contexto.superficiesDosMods.has(doConjunto),
          `a saída da instância não levou o conjunto: solta ${noConjunto.solta}, `
            + `${conjunto.porChave.size} de pé, no mapa ${contexto.superficiesDosMods.has(doConjunto)}`);
        // Pela própria superfície, registrada na instância, sem conjunto.
        const daSuperficie = deSaida();
        const sozinha = new S("mod/s", { instancia: daSuperficie, falar() {} }, { id: "e", tipo: "dialogo" }, {});
        sozinha.marcarSuja(true);
        await daSuperficie.encerrar();
        confere("R4e", sozinha.solta, "a saída da instância não levou a superfície registrada nela");
        confere("R4e", perguntas.length === 0,
          `a saída da instância perguntou antes de descartar: ${JSON.stringify(perguntas)}`);
        confere("R4e", doConjunto.recursos.length === 0 && daSuperficie.recursos.length === 0
          && doConjunto.naoSairam.length === 0 && daSuperficie.naoSairam.length === 0,
        "a saída deixou recurso de pé, ou um que não saiu");
      } finally {
        S.prototype.montarCasca = cascaReal;
        delete contexto.abrirConfirmacao;
        delete contexto.superficiesDosMods;
        delete contexto.palcosDeSuperficie;
      }
    }

    terminar();
  })().catch((erro) => {
    falhas.push(`R2: o roteador lançou — ${erro?.stack || erro}`);
    terminar();
  });
}

function terminar() {
  terminou = true;
  if (falhas.length) {
    console.error("contribuicoes-e-camadas: reprovou");
    for (const f of falhas) console.error(`  ${f}`);
    process.exitCode = 1;
    return;
  }
  console.log(
    "contribuicoes-e-camadas: R1–R6 e a revisão de 26ad0c2 (reabertura, ordem "
    + "inversa, descarte com desenho, mil superfícies, cartão genérico por "
    + `destino) medidos no index.html real (#palco-de-camadas dentro de #${camadas.parentNode.id}) `
    + "e no roteador real; V14 mede a troca de páginas e a volta à navegação nativa.",
  );
}
