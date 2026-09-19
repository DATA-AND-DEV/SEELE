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
//   SeeleUI.aoEvento(fn)                 — receber o que a pessoa faz
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

async function comecar() {
  // O tema é pedido uma vez, e o produto o tira sozinho quando este MOD sai.
  await tentar("tema", () => SeeleUI.tema({ acento: "#6BFFB6" }), null);
  estado.sessao = Boolean(await tentar("sessão", () => SeeleMods.snapshot(), null));
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
