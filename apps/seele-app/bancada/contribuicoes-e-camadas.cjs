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

const documento = {
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
    dialogo.raiz.ouvintes === 1,
    `reabrir registrou ${dialogo.raiz.ouvintes} ouvintes de teclado; um é o certo`,
  );

  S.prototype.soltarFoco.call(dialogo);
  confere(
    "R1c · reabrir",
    fundo.inert === false,
    "fechar depois de reabrir deixou o fundo inerte: a aplicação fica travada sem nada na tela para explicar",
  );
  confere("R1c · reabrir", dialogo.raiz.ouvintes === 0, "sobrou ouvinte de teclado depois de fechar");
  dialogo.camada.remove();
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
    renderer: { soltar() { superficie.soltou = true; } },
    soltou: false,
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
contexto.RegiaoDeMod = class {
  constructor(id, dono, raiz) { this.id = id; this.dono = dono; this.raiz = raiz; }
  aplicar() { return 0; }
  soltar() { soltouRenderer += 1; }
};
contexto.PERFIS_DE_RENDER = { cartao: {}, superficie: {} };
contexto.elemento = (tag) => new No(tag, "");
contexto.donoDaRegiao = (mod, instancia) => ({ instancia, falar() {} });
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

// ------------------------------------- R3 · os pontos aceitam o que aplicam

{
  const registro = new R();
  const dono = new I("mod/a", "a", 7, {});
  for (const [ponto, regra] of Object.entries(PONTOS)) {
    for (const modo of regra.modos) {
      let erro = null;
      try {
        registro.registrar({ id: "mod/a" }, dono, { ponto, modo, alvo: "1" });
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

    terminar();
  })().catch((erro) => {
    falhas.push(`R2: o roteador lançou — ${erro?.stack || erro}`);
    terminar();
  });
}

function terminar() {
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
    + "e no roteador real.",
  );
}
