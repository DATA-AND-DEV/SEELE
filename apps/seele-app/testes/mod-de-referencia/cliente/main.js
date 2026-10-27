// A metade de janela do MOD de referência — ADR 0049, na API 6.
//
// Este arquivo é um **vetor do repositório**, e não um exemplo de documentação:
// a bateria o carrega e cobra que cada coisa que ele chama exista do lado do
// produto **na API que ele declara**, e que cada coisa que essa API oferece
// seja chamada aqui. Se a API mudar sem que este arquivo mude junto, a bateria
// reprova aqui antes de alguém publicar um guia que promete o que sumiu.
//
// Ele exercita **tudo** o que a API 6 oferece, e nada além:
//
//   SeeleMods.request(id, canal, valor)  — perguntar à metade de servidor
//                                          (canal é um identificador; zero
//                                           quer dizer «nenhum canal»)
//   SeeleMods.acao(id, canal, acao, v)   — uma ação declarada (API 7,
//                                          `acoes`); quem autoriza é o servidor
//   SeeleUI.retrato()                    — o que a janela sabe, por lista
//                                          branca (no lugar do `snapshot`)
//   SeeleUI.tema(valores, { previa })    — a prévia de cor, numa superfície
//   SeeleUI.aoEvento(fn)                 — o que a pessoa faz, a tela que
//                                          muda e o aviso do servidor
//   SeeleUI.pedaco(arquivo, inicio)      — ler um arquivo que alguém escolheu
//   SeeleUI.enviar(arquivo, token)       — mandá-lo ao volume da metade de
//                                          servidor
//   SeeleUI.soltar(arquivo)              — devolver esse arquivo agora
//   SeeleUI.capacidades()                — o que **esta** API oferece
//   SeeleUI.superficies.criar(descricao) — uma janela, folha, página ou aviso
//   punho.titulo / classes / montar      — o nome, a folha e o conteúdo dela
//   punho.atualizar(chave, arvore)       — uma parte dela, numa mensagem própria
//   punho.formulario.erros(mapa)         — a recusa do servidor de volta ao campo
//   punho.mostrar / ocultar / suja       — visibilidade e alterações não gravadas
//   punho.fechar / descartar             — tirar do palco, e acabar com ela
//   SeeleUI.superficies.avisar(texto)    — a superfície de vida curta
//   SeeleUI.contribuicoes.registrar(p)   — entrar num ponto ou numa região
//   SeeleUI.contribuicoes.revogar(h)     — e sair dele
//   SeeleUI.contribuicoes.atualizar(h, c) — trocar o conteúdo sem sair
//
// **O que a 6 tirou não aparece aqui** (rascunho da API 6, §2; decisão do dono
// de 08/10): `regiao`, `cartoes` e o `snapshot` inteiro. O desenho principal
// mora numa superfície que não prende a sessão, o cartão de uma pessoa é o
// ponto `pessoa.cartao`, e a região é um ponto como os outros.
//
// Não há `document`, `window` nem o global do Tauri aqui dentro: um worker não
// os tem, e é isso que faz `terminate()` ser garantia em vez de pedido.
//
// # A fatia
//
// As quatro primitivas numa experiência só, porque separadas elas não provam a
// parte difícil — que é como elas convivem:
//
//   **campo + gravação**   o que se digita vai ao servidor, que decide se pode
//   **atualização**        o painel é redesenhado a cada tecla, e o foco fica
//   **arraste**            o traço nasce de pontos e volta declarado
//   **mídia**              o som é do produto: ele monta, toca e solta

const EU = "seele/referencia";

// **Canal zero: esta pergunta não é sobre um canal.** O terceiro argumento de
// `request` é um identificador de canal, e não o nome de uma operação — a
// operação vai no objeto. Um MOD de escopo de servidor manda zero, e o servidor
// entrega `channel: null` à metade de servidor, sem exigir que haja um canal
// de texto aberto na janela.
const SEM_CANAL = 0;

/** O identificador do painel deste MOD: a superfície onde ele desenha. */
const PAINEL = "referencia-painel";

/** O que este MOD sabe agora. Redesenhar é uma função disto, e de nada mais. */
const estado = {
  sessao: false,
  vezes: 0,
  // O que o servidor confirmou ter gravado, que é diferente do que está na
  // caixa: enquanto a gravação não volta, os dois divergem, e é **essa**
  // diferença que um campo mal feito apagaria.
  gravado: "",
  aviso: "",
  tocando: false,
  // Um traço em andamento, e os que já terminaram.
  riscando: null,
  tracos: [],
  // O arquivo que alguém escolheu: o identificador, o que o produto provou
  // sobre ele, e quanto já foi lido. Os bytes nunca estão aqui inteiros.
  escolhido: null,
  lidos: 0,
  // O painel onde este MOD desenha, a janela de prova do ciclo de vida, e os
  // punhos das contribuições que ele registrou.
  painel: null,
  janela: null,
  contribuicoes: [],
  // O que a tela disse por último: o canal aberto, a sala, e o aviso do
  // servidor. Um MOD da 6 ouve isso por evento, e não relendo o retrato.
  tela: "",
};

/** Pede ao servidor, e devolve o erro como texto em vez de deixá-lo subir. */
async function aoServidor(pedido) {
  try {
    const resposta = await SeeleMods.request(EU, SEM_CANAL, pedido);
    if (resposta?.erro) return { erro: String(resposta.erro) };
    return resposta ?? {};
  } catch (falha) {
    return { erro: String(falha?.message ?? falha) };
  }
}

/**
 * O que a pessoa vê, declarado inteiro a cada vez, no painel deste MOD.
 *
 * **Inteiro, e não em pedaços.** O produto reconcilia por chave: o que não
 * mudou não é tocado, e quem está digitando não perde o foco. Um MOD que
 * tentasse mandar só a diferença estaria refazendo esse trabalho pior.
 */
async function desenhar() {
  const painel = estado.painel;
  if (!painel) return;
  await painel.montar([
    { forma: "titulo", chave: "t", dentro: "REFERÊNCIA" },
    {
      forma: "texto",
      chave: "s",
      dentro: `sessão: ${estado.sessao ? "de pé" : "fora"} · vezes: ${estado.vezes}`,
    },
    estado.tela ? { forma: "texto", chave: "tela", dentro: estado.tela } : null,
    {
      forma: "campo",
      chave: "nome",
      rotulo: "APELIDO NO SERVIDOR",
      // O valor declarado é o **gravado**, e não o digitado: é o servidor quem
      // diz o que existe. O produto não o aplica enquanto a caixa tem foco, e
      // por isso os dois podem divergir sem atrapalhar ninguém.
      valor: estado.gravado,
      // **E o inicial também é o gravado.** Sem ele, o inicial é o valor da
      // primeira montagem do campo, e não muda quando este MOD regrava o
      // valor: depois da primeira gravação o painel ficaria sujo para sempre,
      // e fechar perguntaria «sem gravar?» de algo gravado (a MESA, 15/10).
      inicial: estado.gravado,
    },
    { forma: "botao", chave: "gravar", dentro: "GRAVAR" },
    estado.aviso ? { forma: "texto", chave: "aviso", dentro: estado.aviso } : null,
    {
      forma: "tela",
      chave: "risco",
      largura: 220,
      altura: 120,
      tracos: estado.riscando ? [...estado.tracos, estado.riscando] : estado.tracos,
    },
    { forma: "botao", chave: "limpar", dentro: "LIMPAR" },
    {
      forma: "midia",
      chave: "toque",
      fonte: "som/toque.wav",
      descricao: "um toque curto",
      tocando: estado.tocando,
    },
    { forma: "botao", chave: "tocar", dentro: estado.tocando ? "PARAR" : "TOCAR" },
    // A pessoa escolhe; o produto media. Este MOD recebe um número e lê os
    // bytes em pedaços — ele não vê caminho, pasta nem nome de arquivo.
    { forma: "arquivo", chave: "anexo", dentro: "ESCOLHER ARQUIVO" },
    estado.escolhido
      ? {
          forma: "texto",
          chave: "escolhido",
          dentro: `escolhido: ${estado.escolhido.tipo} · ${estado.escolhido.bytes} bytes · lidos ${estado.lidos}`,
        }
      : null,
    estado.escolhido
      ? { forma: "acoes", chave: "do-arquivo", dentro: [
          { forma: "botao", chave: "enviar", dentro: "ENVIAR AO SERVIDOR" },
          { forma: "botao", chave: "soltar", dentro: "SOLTAR ARQUIVO" },
        ] }
      : null,
  ]);
}

/** Grava o que está na caixa, e diz o que o servidor respondeu. */
async function gravar(valor) {
  estado.aviso = "gravando…";
  await desenhar();
  const resposta = await aoServidor({ op: "gravar", apelido: valor });
  if (resposta.erro) {
    // **A recusa é dita.** Um servidor que recusa a escrita por permissão não
    // pode virar um campo que não guarda e não explica.
    estado.aviso = resposta.erro;
  } else {
    estado.gravado = String(resposta.apelido ?? "");
    estado.vezes = Number(resposta.vezes ?? estado.vezes);
    estado.aviso = "gravado";
  }
  await desenhar();
}

/** O que a pessoa digitou desde o último desenho, ainda não gravado. */
let digitado = "";

SeeleUI.aoEvento((evento) => {
  switch (evento.nome) {
    case "campo":
      digitado = String(evento.valor ?? "");
      // Redesenhar **a cada tecla** é o caso difícil de propósito: é assim que
      // se descobre se a atualização incremental preserva o foco. Um MOD que
      // esperasse o `blur` para redesenhar esconderia o defeito.
      estado.aviso = digitado === estado.gravado ? "" : "não gravado";
      desenhar();
      break;
    case "botao":
      if (evento.chave === "gravar") gravar(digitado);
      if (evento.chave === "limpar") {
        estado.tracos = [];
        estado.riscando = null;
        desenhar();
      }
      if (evento.chave === "tocar") {
        estado.tocando = !estado.tocando;
        desenhar();
      }
      if (evento.chave === "enviar") enviarOArquivo();
      if (evento.chave === "soltar") soltarOArquivo();
      break;
    case "traco":
      if (evento.fase === "comecou") estado.riscando = [{ x: evento.x, y: evento.y }];
      else if (evento.fase === "moveu" && estado.riscando) {
        estado.riscando.push({ x: evento.x, y: evento.y });
      } else if (evento.fase === "terminou" && estado.riscando) {
        estado.riscando.push({ x: evento.x, y: evento.y });
        estado.tracos.push(estado.riscando);
        estado.riscando = null;
      }
      desenhar();
      break;
    case "arquivo":
      // **Cancelar é uma resposta**: `null` quer dizer que a pessoa fechou o
      // seletor, e não que o produto ficou calado.
      estado.escolhido = evento.arquivo ?? null;
      estado.lidos = 0;
      estado.aviso = evento.arquivo ? "" : (evento.porque ?? "nenhum arquivo escolhido");
      if (estado.escolhido) lerUmPedaco();
      else desenhar();
      break;
    case "fechar-pedido":
      // **O produto avisa, e não obedece.** Com alterações não gravadas, a
      // primeira tentativa de fechar vira este evento **e** a confirmação do
      // produto. Um MOD que ignorasse o evento não prende ninguém na janela.
      estado.aviso = `a janela pediu para fechar (${evento.porque})`;
      desenhar();
      break;
    case "fechar":
      // Ela já saiu do palco quando isto chega. O punho continua válido: é
      // `mostrar` que a traz de volta, e `descartar` que acaba com ela.
      estado.aviso = evento.descartou ? "janela fechada, alterações descartadas" : "janela fechada";
      desenhar();
      break;
    case "contribuicao":
      // **Recusa contada, e não silêncio.** Uma declaração que não coube no
      // orçamento do ponto chega aqui em vez de simplesmente não aparecer.
      estado.aviso = `contribuição em ${evento.ponto}: ${evento.recusados} nó(s) recusados`;
      desenhar();
      break;
    case "acao":
      // O botão é do produto e o rótulo é deste MOD; o que volta é o ID real
      // que o produto escreveu nele, e nunca o texto que nós desenhamos.
      estado.aviso = `ação ${evento.acao} · pessoa=${evento.pessoa || "-"} canal=${evento.canal || "-"}`;
      desenhar();
      break;
    case "midia":
      // O estado da mídia vem do produto, e não é presumido: um `tocando` que
      // o navegador recusou é um botão mentindo sobre o que está acontecendo.
      estado.tocando = evento.estado === "tocando";
      if (evento.estado === "recusada" || evento.estado === "falhou") {
        estado.aviso = `som: ${evento.estado}`;
      }
      desenhar();
      break;
    // ---- a tela, que a 6 conta por evento (rascunho §7.2) ----
    case "canal":
      estado.tela = `canal aberto: ${evento.canal ?? "nenhum"}`;
      desenhar();
      break;
    case "sala":
      estado.tela = evento.sala == null ? "fora de sala" : `na sala ${evento.sala}`;
      desenhar();
      break;
    case "pessoas":
      estado.tela = evento.entrou != null ? `entrou: ${evento.entrou}` : `saiu: ${evento.saiu}`;
      desenhar();
      break;
    case "tema":
      // A prévia e o tema do servidor dizem o que a casca fez com eles: um
      // tema recusado por contraste é dito, e não engolido.
      estado.aviso = `tema: ${evento.estado}${evento.previa ? " (prévia)" : ""}`;
      desenhar();
      break;
    // ---- o aviso da metade de servidor (rascunho §7.3) ----
    case "servidor":
      estado.tela = `o servidor avisou: ${JSON.stringify(evento.carga ?? null)}`;
      desenhar();
      break;
    default:
      // `fala` e `lugar` chegam muitas vezes por segundo, e este vetor não
      // desenha nada a partir deles: ouvir não obriga a redesenhar.
      break;
  }
});

/**
 * Um passo da subida que **não derruba a subida**.
 *
 * A razão está escrita numa medição: a versão anterior fazia
 * `estado.sessao = Boolean(await SeeleMods.snapshot())` direto, e quando o MOD
 * subia antes de a sessão estar de pé o `snapshot` rejeitava. A função `async`
 * morria ali, na segunda linha, e o MOD nunca desenhava — sem uma palavra.
 *
 * O produto passou a **dizer** a rejeição sem tratamento, que era o outro lado
 * do mesmo defeito. Este lado é o do MOD: cada passo da subida responde por si,
 * e o que falha vira aviso na tela em vez de um MOD que não aparece.
 */
async function tentar(o_que, passo, padrao) {
  try {
    return await passo();
  } catch (falha) {
    estado.aviso = `${o_que}: ${String(falha?.message ?? falha)}`;
    return padrao;
  }
}

/**
 * Lê um pedaço do que foi escolhido, e conta quanto já leu.
 *
 * Em pedaços porque o arquivo pode ter dez megabytes: um MOD que pedisse tudo
 * de uma vez esbarraria no teto de mensagem, e o teto existe para que um MOD
 * não encha a memória de quem está numa conversa.
 */
async function lerUmPedaco() {
  const escolhido = estado.escolhido;
  if (!escolhido) return;
  const pedaco = await tentar("ler o arquivo", () => SeeleUI.pedaco(escolhido.id, estado.lidos), "");
  // O base64 cresce um terço sobre os bytes: quatro caracteres por três bytes.
  estado.lidos += Math.floor((pedaco.length / 4) * 3);
  await desenhar();
}

/**
 * Manda o arquivo inteiro ao volume da metade de servidor, sem passar pela
 * mensagem (rascunho, §8; ADR 0048).
 *
 * **Quem abre a porta é o servidor.** A metade de servidor espera um token
 * (`volume.esperar`), com o caminho, os tipos e o prazo; a janela só manda o
 * arquivo que a pessoa escolheu, com aquele token. Um token que o servidor não
 * pediu não abre nada.
 */
async function enviarOArquivo() {
  const escolhido = estado.escolhido;
  if (!escolhido) return;
  const porta = await aoServidor({ op: "esperar-envio" });
  if (porta.erro || !porta.token) {
    estado.aviso = `enviar: ${porta.erro ?? "o servidor não abriu a porta"}`;
    await desenhar();
    return;
  }
  const enviado = await tentar("enviar o arquivo", () => SeeleUI.enviar(escolhido.id, porta.token), false);
  if (enviado !== false) estado.aviso = "arquivo no volume do servidor";
  await desenhar();
}

/** Devolve o arquivo agora, sem esperar a saída da sessão. */
async function soltarOArquivo() {
  const escolhido = estado.escolhido;
  if (!escolhido) return;
  await tentar("soltar o arquivo", () => SeeleUI.soltar(escolhido.id), null);
  estado.escolhido = null;
  estado.lidos = 0;
  estado.aviso = "arquivo devolvido";
  await desenhar();
}

// ------------------------------------------- os pontos, as regiões e as camadas

/**
 * Os pontos onde um MOD entra na interface do **SEELE**, com o modo de cada
 * um, e uma região.
 *
 * A lista está aqui inteira de propósito. A revisão de 20/09/2026 encontrou
 * oito pontos que aceitavam registro e não produziam efeito nenhum, e o que
 * torna isso visível é um vetor que os registra todos: o que não for aplicado
 * aparece como um ponto sem nada na tela, numa homologação nativa, em vez de
 * aparecer numa revisão meses depois.
 *
 * `alvo` é preenchido na hora, com um ID de verdade do retrato — um ponto por
 * alvo sem alvo nenhum vale para todos, e «todos» não é o que se quer provar.
 * **O cartão de uma pessoa é um ponto**, e não mais a chamada `cartoes` que a
 * API 5 tinha; e **a região é um ponto**, e não mais a chamada `regiao`.
 */
const PONTOS_DO_VETOR = [
  { ponto: "pessoa.identidade", modo: "adicionar", porPessoa: true, rotulo: "" },
  { ponto: "pessoa.cartao", modo: "adicionar", porPessoa: true, rotulo: "" },
  { ponto: "pessoa.detalhes", modo: "adicionar", porPessoa: true, rotulo: "" },
  { ponto: "pessoa.acoes", modo: "adicionar", porPessoa: true, rotulo: "VER" },
  { ponto: "canal.item", modo: "adicionar", porCanal: true, rotulo: "" },
  { ponto: "canal.cabecalho", modo: "adicionar", porCanal: true, rotulo: "FICHA" },
  { ponto: "compositor.ferramentas", modo: "adicionar", rotulo: "DADOS" },
  { ponto: "sala.acoes", modo: "adicionar", porSala: true, rotulo: "MESA" },
  { ponto: "servidor.navegacao", modo: "adicionar", rotulo: "REFERÊNCIA" },
  { ponto: "servidor.aparencia", modo: "substituir", rotulo: "" },
  { ponto: "status.barra", modo: "adicionar", rotulo: "" },
];

/**
 * O painel onde este MOD desenha: um diálogo **que não prende a sessão**.
 *
 * Na 5 o desenho morava na região do MOD; na 6 a região é um ponto, e o que
 * tem campo, tela e mídia mora numa superfície. Não modal, porque um MOD de
 * referência aberto a sessão inteira não pode adormecer o resto dela.
 */
async function abrirOPainel() {
  const painel = await tentar("criar o painel", () => SeeleUI.superficies.criar({
    id: PAINEL,
    tipo: "dialogo",
    modal: false,
    titulo: "Referência",
  }), null);
  estado.painel = painel;
}

/**
 * Uma janela do produto, com o ciclo de vida inteiro exercitado.
 *
 * **O produto monta a casca.** O cabeçalho com o título, a origem — o `id`
 * deste MOD, escrito pelo SEELE —, o botão de sair, o foco contido e o retorno
 * dele ao acionador são do produto. O que este arquivo declara é só o miolo,
 * pela mesma gramática dos pontos.
 */
async function exercitarASuperficie() {
  const janela = await tentar("criar a janela", () => SeeleUI.superficies.criar({
    id: "referencia-janela",
    tipo: "dialogo",
    titulo: "Referência",
    tamanho: { largura: 480 },
    // Com alterações não gravadas, fechar vira uma pergunta **do produto**,
    // sempre, sem opção a declarar — e a resposta da pessoa vence. Um MOD não
    // veta a saída da sessão, que descarta sem perguntar.
    focoInicial: "nota",
  }), null);
  if (!janela) return;
  estado.janela = janela;

  // O nome acessível pode mudar depois de aberta: é o que um editor faz ao
  // abrir um documento diferente na mesma janela.
  await tentar("titulo", () => janela.titulo("Referência · vetor"), null);

  // **Classes, e não texto de CSS.** Cada propriedade é validada uma a uma e o
  // seletor é escrito pelo produto, prefixado com a identidade desta
  // superfície: duas superfícies com a classe `aviso` não se alcançam, e
  // nenhuma das duas alcança nada do SEELE.
  await tentar("classes", () => janela.classes({
    aviso: {
      base: { cor: "#6BFFB6", corpo: 12, peso: "forte" },
      // Estado é seletor, e seletor é regra: sem isto, um MOD precisaria de um
      // evento por movimento do mouse para mudar uma cor.
      sobre: { cor: "#FFFFFF" },
      // A consulta é do **contêiner**, e não da janela: um painel que alguém
      // arrastou para 300 px continua numa janela de 1400.
      consultas: [{ ateLargura: 360, estilo: { corpo: 11 } }],
    },
  }), null);

  await tentar("montar", () => janela.montar([
    { forma: "titulo", chave: "ti", dentro: "O QUE ESTA JANELA PROVA" },
    { forma: "texto", chave: "nota", classe: "aviso", dentro: "a casca é do produto; o miolo é do MOD" },
    { forma: "campo", chave: "rascunho", rotulo: "ANOTAÇÃO", valor: "" },
    { forma: "acoes", chave: "fixas", fixas: true, dentro: [
      { forma: "botao", chave: "gravar-janela", dentro: "GRAVAR" },
    ] },
  ]), null);

  // **Uma parte, e não a janela inteira** (rascunho da API 6, §12): a nota
  // troca sozinha, numa mensagem própria. O teto que vale é o de nós da
  // superfície, e não o de 12 KiB da mensagem.
  await tentar("atualizar uma parte", () => janela.atualizar("nota",
    { forma: "texto", chave: "nota", classe: "aviso", dentro: "a casca é do produto; o miolo é do MOD, por partes" }), null);

  // **A recusa do servidor volta ao campo que a causou** (rascunho da API 6,
  // §5): o campo ganha a frase, e `{}` limpa.
  await tentar("erros do formulário", () => janela.formulario.erros({ rascunho: "o servidor recusou a anotação" }), null);
  await tentar("limpar os erros", () => janela.formulario.erros({}), null);

  // **A prévia de cor é da superfície** (rascunho §9, B4): na 6 a janela só
  // pinta a prévia, dentro da superfície pedida; o tema do servidor é da
  // metade de servidor (`mundo.tema`). Sair da superfície tira a prévia.
  await tentar("prévia de tema", () => SeeleUI.tema({ acento: "#6BFFB6" }, { previa: true, superficie: "referencia-janela" }), null);

  // Mostrar e ocultar são o par que a revisão encontrou quebrado: `ocultar`
  // mantém a janela montada, e `mostrar` a traz de volta ao documento mesmo
  // depois de `fechar`.
  await tentar("mostrar", () => janela.mostrar(), null);
  await tentar("suja", () => janela.suja(true), null);
  await tentar("ocultar", () => janela.ocultar(), null);
  await tentar("mostrar de novo", () => janela.mostrar(), null);

  // E o aviso: uma superfície de vida curta, que não exige montar nada.
  await tentar("avisar", () => SeeleUI.superficies.avisar("o vetor subiu", "normal"), null);
}

/** Registra o vetor em todos os pontos, e solta um deles para provar a volta. */
async function exercitarAsContribuicoes(retrato) {
  // **O retrato é por lista branca** (rascunho §6): `eu`, `pessoas`, `salas`
  // e `canais`, e nada que a lista não nomeie.
  const pessoa = retrato?.pessoas?.[0]?.id ?? retrato?.eu ?? null;
  const canal = retrato?.canais?.find((c) => c.aberto)?.id ?? retrato?.canais?.[0]?.id ?? null;
  const sala = retrato?.salas?.[0]?.id ?? null;

  for (const alvo of PONTOS_DO_VETOR) {
    const destino = alvo.porPessoa ? pessoa : alvo.porCanal ? canal : alvo.porSala ? sala : null;
    // Um ponto por alvo sem alvo não é registrado: ele valeria para **todos**,
    // e o que se quer provar é o alvo certo.
    if ((alvo.porPessoa || alvo.porCanal || alvo.porSala) && destino === null) continue;
    const handle = await tentar(`contribuir em ${alvo.ponto}`, () => SeeleUI.contribuicoes.registrar({
      ponto: alvo.ponto,
      modo: alvo.modo,
      alvo: destino,
      prioridade: 0,
      rotulo: alvo.rotulo,
      nomeAcessivel: alvo.rotulo ? `${alvo.rotulo} no vetor de referência` : "",
      acaoPrincipal: alvo.rotulo ? "abrir" : "",
      conteudo: [{ forma: "texto", chave: "c", dentro: "·ref" }],
    }), null);
    if (handle) estado.contribuicoes.push({ ponto: alvo.ponto, handle: handle.handle ?? handle });
  }

  // **E a troca sem revogar.** O conteúdo muda no mesmo nó, e a linha não
  // volta à nativa no meio (tarefa 5 da F3, Step 6).
  const primeira = estado.contribuicoes[0];
  if (primeira) {
    await tentar("atualizar", () => SeeleUI.contribuicoes.atualizar(primeira.handle,
      [{ forma: "texto", chave: "c", dentro: "·ref·2" }]), null);
  }

  // **E a volta.** Revogar precisa devolver a apresentação nativa — recalculada
  // do estado de agora, e não restaurada de um desenho guardado.
  const ultima = estado.contribuicoes.pop();
  if (ultima) {
    await tentar("revogar", () => SeeleUI.contribuicoes.revogar(ultima.handle), null);
  }
}

/** Fecha e acaba com a janela, que é o que a saída faria sozinha. */
async function encerrarASuperficie() {
  const janela = estado.janela;
  if (!janela) return;
  // Sem alterações não gravadas: com elas, `fechar` vira a pergunta do produto.
  await tentar("limpar o suja", () => janela.suja(false), null);
  await tentar("fechar", () => janela.fechar("pedido-do-mod"), null);
  await tentar("descartar", () => janela.descartar(), null);
  estado.janela = null;
}

async function comecar() {
  const retrato = await tentar("sessão", () => SeeleUI.retrato(), null);
  estado.sessao = Boolean(retrato);

  // **O que esta API oferece, perguntado em vez de presumido.** Na 6, as
  // superfícies e as contribuições estão sempre lá; perguntar continua sendo
  // o que deixa o mesmo arquivo dizer, no quintal, o que ele achou.
  const tem = new Set(SeeleUI.capacidades());
  if (tem.has("superficies")) await abrirOPainel();
  if (tem.has("superficies")) await exercitarASuperficie();
  if (tem.has("contribuicoes")) await exercitarAsContribuicoes(retrato);
  if (tem.has("superficies")) await encerrarASuperficie();
  // **O que a janela viu vai para o quintal**, que é lido de fora sem
  // automação de acessibilidade. É o que separa «a mensagem saiu» de «a marca
  // foi aceita» numa homologação nativa.
  await tentar(
    "anotar capacidades",
    () => aoServidor({ op: "anotar", chave: "capacidades", valor: [...tem].join(",") }),
    null,
  );
  await tentar(
    "anotar contribuicoes",
    () => aoServidor({ op: "anotar", chave: "contribuicoes", valor: estado.contribuicoes.map((c) => c.ponto).join(",") }),
    null,
  );

  const inicial = await aoServidor({ op: "contar" });
  estado.vezes = Number(inicial.vezes ?? 0);
  estado.gravado = String(inicial.apelido ?? "");
  digitado = estado.gravado;
  // E um aviso da metade de servidor, que volta por `aoEvento` como `servidor`.
  await aoServidor({ op: "avisar" });
  // **A ação declarada** (API 7, capacidade `acoes`). Este vetor não declara
  // `permissoes`, e o servidor responde `acao-nao-declarada`: a recusa chega
  // como valor, e o que voltou vai ao quintal. O caminho que passa está em
  // `testes/mod-de-acoes`, provado por `acoes_de_mod.rs`.
  if (tem.has("acoes")) {
    const acao = await tentar("acao", () => SeeleMods.acao(EU, SEM_CANAL, "sem-declaracao", {}), null);
    await tentar(
      "anotar acao",
      () => aoServidor({ op: "anotar", chave: "acao", valor: String(acao?.recusa ?? acao?.erro ?? "") }),
      null,
    );
  }
  await desenhar();
}

// **A subida inteira também responde por si.** Um `catch` aqui é a diferença
// entre um MOD que explica o que deu errado e um que some.
comecar().catch((falha) => {
  estado.aviso = `não subiu: ${String(falha?.message ?? falha)}`;
  desenhar().catch(() => {});
});
