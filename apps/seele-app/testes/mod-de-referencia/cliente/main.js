// A metade de janela do MOD de referência — ADR 0049.
//
// Este arquivo é um **vetor do repositório**, e não um exemplo de documentação:
// a bateria o carrega e cobra que cada coisa que ele chama exista do lado do
// produto. Se a API mudar sem que este arquivo mude junto, a bateria reprova
// aqui antes de alguém publicar um guia que promete o que sumiu.
//
// Ele exercita **tudo** o que a API oferece, e nada além:
//
//   SeeleMods.request(id, canal, valor)  — perguntar à metade de servidor
//                                          (canal é um identificador; zero
//                                           quer dizer «nenhum canal»)
//   SeeleMods.snapshot()                 — o estado que a janela já tem
//   SeeleUI.regiao(conteudo)             — desenhar, declarando
//   SeeleUI.tema(valores)                — pedir cor, dentro da sessão
//   SeeleUI.marcas(marcas)               — marcar pessoas na lista do produto
//   SeeleUI.aoEvento(fn)                 — receber o que a pessoa faz
//   SeeleUI.pedaco(arquivo, inicio)      — ler um arquivo que alguém escolheu
//   SeeleUI.soltar(arquivo)              — devolver esse arquivo agora
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
//   **atualização**        a região é redesenhada a cada tecla, e o foco fica
//   **arraste**            o traço nasce de pontos e volta declarado
//   **mídia**              o som é do produto: ele monta, toca e solta

const EU = "seele/referencia";

// **Canal zero: esta pergunta não é sobre um canal.** O terceiro argumento de
// `request` é um identificador de canal, e não o nome de uma operação — a
// operação vai no objeto. Um MOD de escopo de servidor manda zero, e o servidor
// entrega `channel: null` à metade de servidor, sem exigir que haja um canal
// de texto aberto na janela.
const SEM_CANAL = 0;

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
 * O que a pessoa vê, declarado inteiro a cada vez.
 *
 * **Inteiro, e não em pedaços.** O produto reconcilia por chave: o que não
 * mudou não é tocado, e quem está digitando não perde o foco. Um MOD que
 * tentasse mandar só a diferença estaria refazendo esse trabalho pior.
 */
async function desenhar() {
  await SeeleUI.regiao([
    { forma: "titulo", chave: "t", dentro: "REFERÊNCIA" },
    {
      forma: "texto",
      chave: "s",
      dentro: `sessão: ${estado.sessao ? "de pé" : "fora"} · vezes: ${estado.vezes}`,
    },
    {
      forma: "campo",
      chave: "nome",
      rotulo: "APELIDO NO SERVIDOR",
      // O valor declarado é o **gravado**, e não o digitado: é o servidor quem
      // diz o que existe. O produto não o aplica enquanto a caixa tem foco, e
      // por isso os dois podem divergir sem atrapalhar ninguém.
      valor: estado.gravado,
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
      ? { forma: "botao", chave: "soltar", dentro: "SOLTAR ARQUIVO" }
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
    case "midia":
      // O estado da mídia vem do produto, e não é presumido: um `tocando` que
      // o navegador recusou é um botão mentindo sobre o que está acontecendo.
      estado.tocando = evento.estado === "tocando";
      if (evento.estado === "recusada" || evento.estado === "falhou") {
        estado.aviso = `som: ${evento.estado}`;
      }
      desenhar();
      break;
    default:
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

async function comecar() {
  // O tema é pedido uma vez, e o produto o tira sozinho quando este MOD sai.
  await tentar("tema", () => SeeleUI.tema({ acento: "#6BFFB6" }), null);
  const retrato = await tentar("sessão", () => SeeleMods.snapshot(), null);
  estado.sessao = Boolean(retrato);

  // **A marca é dado, e quem desenha é o produto.** Este vetor marca a primeira
  // pessoa do retrato para exercitar a única superfície que um MOD alcança fora
  // da região dele. Sem ninguém no retrato, ele manda o conjunto vazio — que é
  // também como se tira uma marca.
  // Alguém numa sala de voz, ou a própria pessoa quando não há sala nenhuma.
  //
  // O `me` existe desde que a sessão existe, e as salas não: sem ele, uma
  // homologação com uma pessoa só provaria apenas o conjunto vazio — que é o
  // caso em que a API não desenha nada, e portanto o caso que menos prova.
  const alguem =
    retrato?.voice_rooms?.flatMap((sala) => sala.people ?? [])?.[0] ??
    (retrato?.me == null ? null : { id: retrato.me });
  let marcou;
  try {
    await SeeleUI.marcas(alguem ? { [alguem.id]: { texto: "REF", cor: "#6BFFB6" } } : {});
    marcou = alguem ? `ok pessoa=${alguem.id}` : "ok vazio";
  } catch (falha) {
    marcou = `recusou: ${String(falha?.message ?? falha)}`;
    estado.aviso = `marcas: ${marcou}`;
  }
  // **O que a janela viu vai para o quintal**, que é lido de fora sem
  // automação de acessibilidade. É o que separa «a mensagem saiu» de «a marca
  // foi aceita» numa homologação nativa.
  await tentar("anotar marcas", () => aoServidor({ op: "anotar", chave: "marcas", valor: marcou }), null);

  const inicial = await aoServidor({ op: "contar" });
  estado.vezes = Number(inicial.vezes ?? 0);
  estado.gravado = String(inicial.apelido ?? "");
  digitado = estado.gravado;
  await desenhar();
}

// **A subida inteira também responde por si.** Um `catch` aqui é a diferença
// entre um MOD que explica o que deu errado e um que some.
comecar().catch((falha) => {
  estado.aviso = `não subiu: ${String(falha?.message ?? falha)}`;
  desenhar().catch(() => {});
});
