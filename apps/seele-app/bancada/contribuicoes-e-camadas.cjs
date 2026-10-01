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
  + "this.R = RegistroDeContribuicoes; this.S = SuperficieDeMod;"
  + "this.I = InstanciaDeMod; this.PONTOS = PONTOS_DE_CONTRIBUICAO;"
  + "this.NATIVO = NATIVO; this.ESTADOS_DE_MOD = ESTADOS_DE_MOD;",
  contexto,
);
const { R, S, I, PONTOS, NATIVO } = contexto;

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
  // **Só leitura, e por alvo.** `resumo` responde pelo alvo vazio, e por isso
  // não via uma substituição registrada para uma pessoa só — que é como todo
  // avatar do PERFIS é registrado. `quemPinta` pergunta a mesma
  // `escolherSubstituicao` para cada alvo que tem contribuição.
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
  constructor(id, dono, raiz) {
    this.id = id;
    this.dono = dono;
    this.raiz = raiz;
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
contexto.cartoesDosMods = new Map();

{
  const base = ler("base.js");
  const inicio = base.indexOf("function montarContribuicao(");
  const fim = base.indexOf("/**\n * As ações que os MODs acrescentaram", inicio);
  confere("R4b · o recorte", inicio >= 0 && fim > inicio, "`montarContribuicao` mudou de forma e o recorte não a achou");
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
  // `escolherSubstituicao` a escolhe, um acréscimo desenha sempre, e um
  // cartão da API 3 desenha quando o MOD está em `modsDeCartaoQueValem`.
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
  contexto.regioesDosMods = new Map([
    [a, { id: "mod/a", ...renderer("a região de mod/a, cujos cartões valem") }],
    [b, { id: "mod/b", ...renderer("a região de mod/b, cujos cartões não valem") }],
  ]);
  contexto.modsDeCartaoQueValem = () => ["mod/a"];
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
    "a região de mod/a, cujos cartões valem": true,
    "a região de mod/b, cujos cartões não valem": false,
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
  const regiaoDeAntes = contexto.regiaoDoMod;
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

  // Cartões recusados, pelo `darCartoesDoMod` de verdade.
  const base = ler("base.js");
  const inicio = base.indexOf("function darCartoesDoMod(");
  const fim = base.indexOf("\n}\n", inicio) + 3;
  confere("R4d · o recorte", inicio >= 0, "`darCartoesDoMod` mudou de forma e o recorte não a achou");
  vm.runInContext(base.slice(inicio, fim), contexto);
  contexto.regiaoDoMod = () => ({ declararCartoes: () => 2 });
  const devolvidos = contexto.darCartoesDoMod({ id: "mod/c" }, {}, { 7: [] });
  confere("R4d · cartões", devolvidos === 2, `o MOD deixou de receber o número de recusas: ${devolvidos}`);
  confere("R4d · cartões", dita("mod/c", "2 nó(s)", "cartões"),
    `os cartões que não couberam não chegaram ao registro: ${JSON.stringify(anotadas)}`);

  contexto.regiaoDoMod = regiaoDeAntes;
  contexto.RegiaoDeMod = rendererDeAntes;
  contexto.registrarNoAnfitriao = registrarDeAntes;
  contexto.cartoesDosMods.clear();
}

// ------------- R3b · a substituição desenha o conteúdo da própria contribuição

{
  const registro = new R();
  const dono = new I("mod/a", "a", 7, {});
  dono.estado = contexto.ESTADOS_DE_MOD.ativa;

  // **Sem `SeeleUI.cartoes`.** É o ponto: um autor que siga o contrato
  // genérico registra `pessoa.cartao/substituir` com `conteudo` e mais nada.
  // `cartoesDosMods` fica vazio de propósito — era ele, e só ele, que o
  // caminho antigo consultava.
  contexto.cartoesDosMods.clear();
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

  // **O legado continua valendo**, porque um pacote de API 3 é executado.
  const legado = new No("div", "");
  contexto.cartoesDosMods.set("mod/a", { cartaoDe: (id) => (String(id) === "12" ? legado : null) });
  const semConteudo = { mod: "mod/a", instancia: dono, ponto: "pessoa.cartao", conteudo: null };
  confere(
    "R3b · o legado",
    contexto.cartaoDeContribuicao(semConteudo, 12) === legado,
    "uma contribuição sem conteúdo deixou de cair em `SeeleUI.cartoes`, que é o caminho da API 3",
  );
  contexto.cartoesDosMods.clear();
}

// ------------- N4 · a escolha de apresentação alcança o caminho legado

{
  // `SeeleUI.cartoes` é o caminho da API 3 para o mesmo ponto que
  // `pessoa.cartao/substituir`. A preferência alcançava só a substituição:
  // escolher «usar apresentação do SEELE» devolvia o nome nativo **e deixava o
  // cartão do MOD logo abaixo**. A validação nativa de 20/09/2026 observou.
  const base = ler("base.js");
  // **As duas funções, e não uma.** Desde a fase M1 a regra mora em
  // `modsDeCartaoQueValem`, que «quem pinta cada lugar» também lê, e
  // `cartoesDaPessoa` só desenha por ela. Recortar só a segunda mediria uma
  // função que chama o que não está no recorte.
  const inicio = base.indexOf("function modsDeCartaoQueValem(");
  const desenha = base.indexOf("function cartoesDaPessoa(", inicio);
  const fim = base.indexOf("\n}", desenha) + 2;
  confere(
    "N4 · o recorte",
    inicio >= 0 && desenha > inicio,
    "`modsDeCartaoQueValem` e `cartoesDaPessoa` deixaram de ser vizinhas, e o recorte não as achou",
  );
  vm.runInContext(base.slice(inicio, fim), contexto);

  const doA = new No("div", "");
  const doB = new No("div", "");
  contexto.cartoesDosMods.clear();
  contexto.cartoesDosMods.set("mod/a", { cartaoDe: () => doA });
  contexto.cartoesDosMods.set("mod/b", { cartaoDe: () => doB });

  let escolhido = "";
  contexto.modPreferidoPara = () => escolhido;

  confere(
    "N4 · automático",
    contexto.cartoesDaPessoa(12).length === 2,
    "sem escolha, os cartões dos dois MODs deixaram de aparecer",
  );

  escolhido = contexto.NATIVO;
  confere(
    "N4 · nativo",
    contexto.cartoesDaPessoa(12).length === 0,
    "«usar apresentação do SEELE» deixou o cartão do MOD pendurado abaixo do nativo",
  );

  escolhido = "mod/b";
  const so = contexto.cartoesDaPessoa(12);
  confere("N4 · provedor", so.length === 1 && so[0] === doB, "escolher um provedor não tirou o cartão do outro");

  escolhido = "";
  contexto.cartoesDosMods.clear();
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
 *   `quemPintaCadaPonto` monta — com `fraseDaMidia`, `fraseDaPreferencia`,
 *   `modsDoCaminhoAntigo` e `preferenciaConsultadaPara`, os vizinhos que ela lê.
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
    [base, "function modsDeCartaoQueValem(", "/**\n * Os cartões desta pessoa"],
    [camada, "function preferenciaConsultadaPara(", "/**\n * Escolhe quem apresenta um ponto"],
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
  // O que a aba lê da página e dos MODs de pé: a preferência desta máquina,
  // os cartões da API 3 e o tema da API 3. O registro é de verdade.
  aba.preferencias = new Map();
  aba.modPreferidoPara = (ponto) => aba.preferencias.get(ponto) ?? "";
  aba.cartoesDosMods = new Map();
  aba.temaDosMods = new Map();
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

  // **Os cartões da API 3**: só os de quem vale, e só as anotações de cartão —
  // a região do mesmo MOD mora no mesmo renderer, e não é de `pessoa.cartao`.
  aba.cartoesDosMods.set("mod/c", anotando({ situacao: "pronta", cartao: true }, { situacao: "carregando", cartao: false }));
  aba.cartoesDosMods.set("mod/d", anotando({ situacao: "carregando", cartao: true }));
  aba.preferencias.set("pessoa.cartao", "mod/c");
  const dosCartoes = aba.midiasDoPonto("pessoa.cartao");
  confere(
    "M1 · a aba · os cartões da API 3",
    dosCartoes.pronta === 1 && dosCartoes.carregando === 0,
    "com mod/c escolhido, a mídia do cartão da API 3 dele não foi contada, ou a da região dele e a dos cartões de "
      + `mod/d, que não valem, foram contadas junto: ${JSON.stringify(dosCartoes)}`,
  );
  aba.preferencias.clear();
  aba.cartoesDosMods.clear();

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
          ...(ponto === "pessoa.avatar" ? { conteudo: { doServidor: { canal: 1, pedido: {} } } } : {}),
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
      mensagem.includes("decorar") && mensagem.includes("API 4"),
      `«${ponto}» não explicou a suspensão de «decorar»: ${mensagem || "aceitou o registro"}`,
    );
  }
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
    await contexto.atenderOMod({ id: "mod/b", api: 4 }, b, {
      tipo: "revogar-contribuicao", n: 1, handle,
    });
    confere("R2 · o dono", registro.porHandle.has(handle), "mod/b removeu a contribuição de mod/a");
    confere("R2 · o dono", respostas.at(-1)?.ok === false, "o roteador respondeu sucesso a uma revogação de outro MOD");

    // **E um pacote de API 3 não alcança uma mensagem da API 4.** O prelúdio
    // omite o método; `seele.postar` emite a mensagem do mesmo jeito, e é por
    // isso que a conferência é do anfitrião.
    await contexto.atenderOMod({ id: "mod/b", api: 3 }, b, {
      tipo: "contribuir", n: 2,
      pedido: { ponto: "servidor.navegacao", rotulo: "API 3" },
    });
    confere("R2 · a versão", respostas.at(-1)?.ok === false, "um pacote de API 3 contribuiu pela mensagem direta");
    confere(
      "R2 · a versão",
      String(respostas.at(-1)?.erro ?? "").includes("API 3"),
      `a recusa não disse que a API do pacote é a razão: ${respostas.at(-1)?.erro}`,
    );

    // E o mesmo pacote declarando API 4 é aceito: a conferência é por versão,
    // e não uma porta fechada.
    await contexto.atenderOMod({ id: "mod/b", api: 4 }, b, {
      tipo: "contribuir", n: 3,
      pedido: { ponto: "servidor.navegacao", rotulo: "API 4" },
    });
    confere("R2 · a versão", respostas.at(-1)?.ok === true, `a API 4 foi recusada: ${respostas.at(-1)?.erro}`);

    // **E A revoga o que é de A.** Sem esta metade, a correção seria uma porta
    // fechada para todo mundo — e uma porta fechada passa no teste de
    // isolamento sem oferecer a capacidade.
    await contexto.atenderOMod({ id: "mod/a", api: 4 }, a, {
      tipo: "revogar-contribuicao", n: 4, handle,
    });
    confere("R2 · o dono", respostas.at(-1)?.ok === true, `mod/a não conseguiu revogar a própria contribuição: ${respostas.at(-1)?.erro}`);
    confere("R2 · o dono", respostas.at(-1)?.valor === true, "a revogação do dono disse que não havia o que tirar");
    confere("R2 · o dono", registro.porHandle.has(handle) === false, "a revogação do dono não tirou a contribuição");
    confere("R2 · o dono", a.recursos.length === 0, "a revogação do dono deixou o descartador retido na instância");

    // **As recusas do roteador chegam ao registro com o id do MOD em campo
    // próprio.** Elas sempre foram ditas ao MOD e já saíam como aviso; o que
    // faltava era o `modId`, que o `registrar_da_janela` escreve como
    // `mod_id=` — o mesmo campo das linhas do Rust. Duas portas, o mesmo
    // `catch` de `atenderOMod`: a contribuição recusada e a região com nós
    // demais, esta pelo `desenharARegiaoDoMod` de verdade.
    {
      const anotadasDoRoteador = [];
      const registrarDeAntes = contexto.registrarNoAnfitriao;
      const regiaoDeAntes = contexto.regiaoDoMod;
      const inicioDaRegiao = base.indexOf("function desenharARegiaoDoMod(");
      const fimDaRegiao = base.indexOf("\n}\n", inicioDaRegiao) + 3;
      confere("R2 · o recorte da região", inicioDaRegiao >= 0,
        "`desenharARegiaoDoMod` mudou de forma e o recorte não a achou");
      vm.runInContext(base.slice(inicioDaRegiao, fimDaRegiao), contexto);
      // Uma região que recusa três nós: é o que o renderer devolve quando a
      // árvore passa do teto, e é o número que `desenharARegiaoDoMod` lança.
      contexto.regiaoDoMod = () => ({ aplicar: () => 3 });
      contexto.registrarNoAnfitriao = (...argumentos) => anotadasDoRoteador.push(argumentos);
      let contribuicaoRespondida;
      let regiaoRespondida;
      try {
        await contexto.atenderOMod({ id: "mod/b", api: 4 }, b, {
          tipo: "contribuir", n: 5,
          pedido: { ponto: "nao.existe", rotulo: "x" },
        });
        contribuicaoRespondida = respostas.at(-1);
        await contexto.atenderOMod({ id: "mod/b", api: 4 }, b, {
          tipo: "regiao", n: 6,
          conteudo: [{ forma: "texto", chave: "t", dentro: "x" }],
        });
        regiaoRespondida = respostas.at(-1);
      } finally {
        contexto.registrarNoAnfitriao = registrarDeAntes;
        contexto.regiaoDoMod = regiaoDeAntes;
      }
      const dita = (...pedacos) => anotadasDoRoteador.some(([onde, texto, nivel, modId]) =>
        onde === "atender-mod" && nivel === "aviso" && modId === "mod/b"
        && pedacos.every((pedaco) => String(texto).includes(pedaco)));
      confere("R2 · a recusa no registro", contribuicaoRespondida?.ok === false,
        "um ponto que não existe foi aceito");
      confere(
        "R2 · a recusa no registro",
        dita("nao.existe"),
        `a contribuição recusada não chegou ao registro com o nível e o id: ${JSON.stringify(anotadasDoRoteador)}`,
      );
      confere("R2 · a região no registro", regiaoRespondida?.ok === false,
        "uma região com nós demais foi respondida como aceita");
      confere(
        "R2 · a região no registro",
        dita("«regiao»", "3 nó(s)"),
        `a região com nós demais não chegou ao registro com o nível e o id: ${JSON.stringify(anotadasDoRoteador)}`,
      );
    }

    // **R4e · o avatar que não montou é dito.** Aqui, dentro do roteador, e não
    // em R4d: a recusa chega numa promessa, e este é o trecho da bancada que
    // espera antes de `terminar`. É o caso de 23/09 — o avatar do PERFIS.
    //
    // Duas chamadas: a que a janela recusa (lança um `Error`) e a que o Rust
    // recusa, que chega como `{ Recusado: { motivo } }` e não como `Error`. Só
    // a segunda separa `motivoDaFalha` de `String(falha?.message ?? falha)` —
    // com esta, o registro voltaria a dizer «[object Object]».
    {
      const regiaoFonte = ler("mods-regiao.js");
      const inicioDoMotivo = regiaoFonte.indexOf("function motivoDaFalha(");
      const fimDoMotivo = regiaoFonte.indexOf("\n}\n", inicioDoMotivo) + 3;
      const inicioDoAvatar = base.indexOf("function avatarContribuido(");
      const fimDoAvatar = base.indexOf("\n}\n", inicioDoAvatar) + 3;
      confere("R4e · o recorte", inicioDoMotivo >= 0 && inicioDoAvatar >= 0,
        "`motivoDaFalha` ou `avatarContribuido` mudou de forma e o recorte não a achou");
      vm.runInContext(regiaoFonte.slice(inicioDoMotivo, fimDoMotivo), contexto);
      vm.runInContext(base.slice(inicioDoAvatar, fimDoAvatar), contexto);
      for (const [pessoa, carregar, motivo] of [
        [12, () => Promise.resolve({ uri: "x:", papel: "som", bytes: 2 }), "não é imagem"],
        [13, () => Promise.reject({ Recusado: { motivo: "fora do teto" } }), "fora do teto"],
      ]) {
        const ditas = [];
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
          falar() {},
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
        try {
          confere("R4e · avatar", contexto.avatarContribuido(pessoa) === null,
            "o avatar devolveu uma imagem antes de ela chegar");
          await new Promise((resolve) => setImmediate(resolve));
        } finally {
          contexto.contribuicoesDosMods = registroDeAntes;
          contexto.donoDaRegiao = donoDeAntes;
          contexto.avisarQueAMidiaMudou = avisarDeAntes;
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
