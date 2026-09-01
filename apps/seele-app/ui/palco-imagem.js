// A imagem de uma tela alheia, decodificada pela própria janela.
//
// # Por que aqui e não no Rust
//
// O que atravessa a ponte são quadros H.264 comprimidos, como saíram do
// codificador do outro lado. Decodificá-los do lado do Rust exigiria o módulo
// do Cisco **em quem só assiste** — e o módulo é o arquivo que não vem no
// pacote por causa da licença. Decodificar aqui usa o decodificador do sistema,
// que a janela alcança pelo `VideoDecoder`: é acelerado por hardware, já está
// em toda máquina, e deixa o módulo sendo assunto só de quem transmite.
//
// O outro caminho — decodificar no Rust e mandar pixels — custaria 3,7 MB por
// quadro atravessando a ponte em JSON. Comprimido, o mesmo quadro tem uns 15 kB.

/** O decodificador da transmissão que está chegando, ou `null`. */
let decodificador = null;

/** Qual transmissão ele está decodificando, para descartar quadro de outra. */
let telaEmCurso = null;

/**
 * Se já chegou um quadro-chave desta transmissão.
 *
 * Um `VideoDecoder` alimentado com um quadro delta antes do primeiro chave
 * entra em erro e não volta — e isso acontece de verdade, não é hipótese: quem
 * entra numa sala onde a transmissão já está correndo pega o fluxo no meio.
 * Descartar até o primeiro chave é o que transforma «o decodificador morreu»
 * em «a imagem demora um instante a aparecer».
 */
let esperandoChave = true;

/** O relógio dos quadros, em microssegundos. */
let carimbo = 0;

/** As medidas que a transmissão anunciou, guardadas até o SPS chegar. */
let medidasDaTela = null;

/** Se `armarPeloSps` está em curso: `isConfigSupported` é assíncrona. */
let armando = false;

/**
 * A string do codec, **lida do SPS que chegou** — nunca suposta.
 *
 * # Por que ler, e não escrever o que se espera
 *
 * Aqui havia `avc1.42e0XX`, com um comentário que dizia: «o perfil é sempre
 * baseline — `codec.rs` escolhe CAVLC justamente para o OpenH264 não subir
 * para High». Era verdade quando foi escrito, e deixou de ser no commit que
 * adotou CABAC: **CABAC não existe em Baseline**, e o codificador subiu para
 * High (`profile_idc` 100) sem que este arquivo soubesse.
 *
 * O resultado é o pior tipo de defeito: o decodificador aceita a configuração,
 * desenha por um tempo, e morre quando encontra o que não foi armado para ler.
 * «Começou funcionando e parou.»
 *
 * A lição não é «corrigir o número». É que **um lado não pode declarar o que o
 * outro decide.** O SPS já viaja em todo quadro-chave e já carrega perfil,
 * restrições e nível — os três bytes que a string precisa. Lê-los é a única
 * versão disto que não pode envelhecer.
 *
 * @param {Uint8Array} bytes um quadro em Annex-B
 * @returns {string|null} `avc1.PPCCLL`, ou `null` se não houver SPS
 */
function codecDoSps(bytes) {
  const hex = (n) => n.toString(16).padStart(2, "0");
  // Annex-B separa os NAL por `00 00 01` ou `00 00 00 01`; o SPS é o tipo 7.
  for (let i = 0; i + 4 < bytes.length; i += 1) {
    if (bytes[i] !== 0 || bytes[i + 1] !== 0) continue;
    let comeco = -1;
    if (bytes[i + 2] === 1) comeco = i + 3;
    else if (bytes[i + 2] === 0 && bytes[i + 3] === 1) comeco = i + 4;
    if (comeco < 0 || comeco + 3 >= bytes.length) continue;
    if ((bytes[comeco] & 0x1f) !== 7) continue;
    // profile_idc, os oito bits de restrição, level_idc — nessa ordem, logo
    // depois do byte de cabeçalho do NAL. É a tabela A-1 do H.264.
    return `avc1.${hex(bytes[comeco + 1])}${hex(bytes[comeco + 2])}${hex(bytes[comeco + 3])}`;
  }
  return null;
}

/**
 * Diz por que a tela alheia não apareceu, **onde dá para ver**.
 *
 * Todo desfecho ruim deste arquivo era um `console.warn` e mais nada. Sete
 * causas diferentes — janela sem `VideoDecoder`, quadro-chave sem SPS, perfil
 * que esta máquina não decodifica, decodificador morto, quadro ilegível —
 * produziam a mesma tela preta muda, e quem estava olhando não tinha como
 * distinguir nenhuma delas nem contá-la a quem pode consertar.
 *
 * O produto já pagou por isso: o defeito de perfil que custou a 0.8.5 em campo
 * chegou como «não funciona», e o que estava no console dizia exatamente qual
 * era. Ninguém abre o console de um aplicativo.
 *
 * O `console.warn` **fica** ao lado: ele carrega o objeto de erro inteiro, que
 * esta frase não tem como carregar.
 */
function naoDeuParaMostrar(frase) {
  const onde = $("palco-falha");
  if (!onde) return;
  onde.textContent = frase;
  onde.hidden = false;
}

/** Apaga a explicação: alguma coisa voltou a funcionar. */
function deuCerto() {
  const onde = $("palco-falha");
  if (onde) onde.hidden = true;
}

/** Desenha um quadro decodificado e o solta. */
function pintar(quadro) {
  // **Aqui e não em `configure`.** Configurar é dizer que se pretende
  // decodificar; um quadro na tela é a prova de que se conseguiu. Uma
  // explicação apagada cedo demais some no instante em que ela é mais
  // verdadeira — entre armar e o primeiro quadro é exatamente onde o defeito
  // de perfil da 0.8.5 morava.
  deuCerto();
  const tela = $("palco-imagem");
  try {
    if (tela.width !== quadro.displayWidth || tela.height !== quadro.displayHeight) {
      tela.width = quadro.displayWidth;
      tela.height = quadro.displayHeight;
    }
    const pincel = tela.getContext("2d");
    if (pincel) pincel.drawImage(quadro, 0, 0);
    tela.hidden = false;
    document.body.dataset.vendoTela = "sim";
  } finally {
    // Sempre, mesmo se o desenho falhar: um `VideoFrame` não fechado segura
    // memória de vídeo, e quinze por segundo esgotam a fila do decodificador
    // em poucos segundos.
    quadro.close();
  }
}

/**
 * Arma o decodificador para uma transmissão que acabou de abrir.
 *
 * Assíncrono porque `isConfigSupported` é: perguntar antes é o que separa «este
 * Mac não decodifica isto» de uma tela preta sem explicação.
 */
async function abrirImagemDaTela(tela, largura, altura) {
  fecharImagemDaTela();
  telaEmCurso = tela;
  esperandoChave = true;
  carimbo = 0;

  if (typeof VideoDecoder === "undefined") {
    console.warn("esta janela não tem VideoDecoder; a tela alheia não será desenhada");
    naoDeuParaMostrar("ESTA JANELA NÃO SABE DECODIFICAR VÍDEO · ATUALIZE O APLICATIVO");
    return;
  }

  // **O decodificador não nasce aqui.** Ele precisa do perfil, e o perfil está
  // no SPS, que só chega com o primeiro quadro-chave. Ver `codecDoSps`.
  medidasDaTela = { largura, altura };
}

/**
 * Arma o decodificador com o perfil que o quadro-chave declarou.
 *
 * Assíncrona porque `isConfigSupported` é — e enquanto ela corre, os quadros
 * que chegam são descartados por `armando`. Não custa nada: já se estava
 * esperando um quadro-chave, e o próximo serve.
 */
async function armarPeloSps(bytes) {
  if (!medidasDaTela) return;
  armando = true;
  const daTela = telaEmCurso;
  try {
    const codec = codecDoSps(bytes);
    if (!codec) {
      console.warn("o quadro-chave veio sem SPS; não dá para saber o perfil");
      naoDeuParaMostrar("O QUADRO NÃO DISSE EM QUE FORMATO VEIO");
      return;
    }
    const config = {
      codec,
      codedWidth: medidasDaTela.largura,
      codedHeight: medidasDaTela.altura,
      // Sem `description`: é o que diz ao decodificador que o fluxo é Annex-B,
      // que é como o OpenH264 entrega e como `Transmissao` põe no fio.
      optimizeForLatency: true,
    };

    const veredito = await VideoDecoder.isConfigSupported(config);
    // A transmissão pode ter trocado enquanto se esperava.
    if (daTela !== telaEmCurso) return;
    if (!veredito.supported) {
      console.warn("esta janela não decodifica", config.codec);
      // O código do perfil junto: é ele que diz a quem conserta **qual**
      // formato esta máquina recusou, e sem ele a frase não ajuda ninguém.
      naoDeuParaMostrar(`ESTA MÁQUINA NÃO DECODIFICA ${config.codec}`);
      return;
    }

    decodificador = new VideoDecoder({
      output: pintar,
      error: (falha) => {
        console.warn("decodificador de tela:", falha);
        naoDeuParaMostrar("O DECODIFICADOR DE VÍDEO PAROU · ESPERANDO O PRÓXIMO QUADRO-CHAVE");
        // Morreu: o próximo quadro-chave arma outro. Não adianta insistir com
        // este — um `VideoDecoder` em erro não volta.
        decodificador = null;
        esperandoChave = true;
      },
    });
    decodificador.configure(config);

    // **E o quadro que armou é entregue, e não jogado fora.**
    //
    // Ele é um quadro-chave por definição — é dele que o SPS saiu — e é
    // exatamente o que um decodificador recém-configurado precisa receber
    // primeiro. Aqui estava escrito «armado, mas ainda sem quadro: o próximo
    // chave é que começa a desenhar», e o próximo chave **não vem sozinho**: o
    // codificador manda um no começo e depois só quando alguém pede.
    //
    // Para quem assistia, a tela ficava preta para sempre, sem erro nenhum —
    // nada tinha falhado. O decodificador estava armado e em silêncio,
    // esperando um quadro que não existia, e todo delta que chegava era pulado
    // pelo `esperandoChave`. Foi assim que o compartilhamento apareceu nos dois
    // sistemas: «sem erro mas tela preta».
    esperandoChave = false;
    entregarAoDecodificador(true, bytes);
  } catch (falha) {
    console.warn("armar o decodificador:", falha);
    naoDeuParaMostrar("NÃO CONSEGUI PREPARAR O VÍDEO NESTA MÁQUINA");
  } finally {
    armando = false;
  }
}

/** Recebe um quadro comprimido em base64 e o entrega ao decodificador. */
function quadroDaTela(tela, chave, base64) {
  if (tela !== telaEmCurso) return;

  let bytes;
  try {
    const cru = atob(base64);
    bytes = new Uint8Array(cru.length);
    for (let i = 0; i < cru.length; i += 1) bytes[i] = cru.charCodeAt(i);
  } catch (falha) {
    console.warn("quadro de tela ilegível:", falha);
    naoDeuParaMostrar("UM QUADRO CHEGOU ILEGÍVEL");
    return;
  }

  // Os bytes são desembrulhados **antes** desta conferência porque é deles que
  // sai o perfil: sem quadro-chave lido, não há decodificador a armar.
  if (!decodificador) {
    if (chave && !armando) armarPeloSps(bytes);
    return;
  }
  entregarAoDecodificador(chave, bytes);
}

/**
 * Entrega um quadro ao decodificador já armado.
 *
 * Separada de `quadroDaTela` porque **quem arma também entrega**: o
 * quadro-chave que trouxe o SPS é o primeiro que o decodificador tem de
 * receber, e antes desta separação ele era lido para descobrir o perfil e
 * depois descartado.
 */
function entregarAoDecodificador(chave, bytes) {
  if (!decodificador) return;
  if (esperandoChave) {
    if (!chave) return;
    esperandoChave = false;
  }

  // Microssegundos, que é a unidade que o `EncodedVideoChunk` pede. Inteiro
  // porque um carimbo fracionário é recusado.
  carimbo = Math.round(performance.now() * 1000);

  try {
    decodificador.decode(
      new EncodedVideoChunk({
        type: chave ? "key" : "delta",
        // Um relógio nosso, e monotônico: o protocolo não carrega carimbo de
        // tempo, e o `VideoDecoder` exige um.
        //
        // **O relógio da máquina, e não um contador de passo fixo.** Aqui havia
        // `carimbo += 33_333`, que é o intervalo de 30 quadros por segundo
        // escrito à mão — e o dia em que quem transmite escolhe 60 é o dia em
        // que esse carimbo passa a andar na metade da velocidade dos quadros
        // que chegam. É a mesma forma do defeito do perfil: um lado supondo o
        // que o outro decide.
        //
        // `performance.now()` cresce sozinho e no ritmo certo qualquer que seja
        // a cadência, sem esta janela precisar saber qual é.
        timestamp: carimbo,
        data: bytes,
      }),
    );
  } catch (falha) {
    console.warn("decode:", falha);
    naoDeuParaMostrar("ESTE VÍDEO NÃO ESTÁ SENDO ACEITO PELO DECODIFICADOR");
  }
}

/** A transmissão acabou: solta o decodificador e apaga a imagem. */
function fecharImagemDaTela() {
  if (decodificador) {
    try {
      decodificador.close();
    } catch (falha) {
      console.warn("fechar o decodificador:", falha);
    }
  }
  decodificador = null;
  telaEmCurso = null;
  esperandoChave = true;
  medidasDaTela = null;
  deuCerto();
  armando = false;
  delete document.body.dataset.vendoTela;
  const tela = $("palco-imagem");
  if (tela) {
    tela.hidden = true;
    // Zerar em vez de só esconder: um canvas guarda o último quadro, e ele
    // reapareceria por um instante na próxima transmissão — a tela de outra
    // pessoa, de outra hora, piscando antes da que se quer ver.
    tela.width = 0;
    tela.height = 0;
  }
}

// ------------------------------------------------------------------ cinema

/**
 * Se a janela está em tela cheia.
 *
 * Guardado aqui e não lido do `document`: quem manda é a janela do sistema, e
 * `document.fullscreenElement` fica `null` numa janela que o Tauri pôs em tela
 * cheia — são dois mecanismos diferentes, e ler o errado faria o botão dizer o
 * contrário do que está acontecendo.
 */
let noCinema = false;

/**
 * Marca o caminho da imagem até o `<body>` e esconde tudo o que fica de fora.
 *
 * Sobe nó a nó em vez de confiar num seletor com a estrutura escrita dentro
 * dele. A primeira versão da tela cheia usava `#vista-chamada > *:not(#palco)`,
 * e o palco está **três** níveis abaixo — a regra escondia a caixa que o
 * continha, e a tela cheia abria vazia. Uma `<div>` a mais no HTML quebraria a
 * regra de novo, e quebraria em silêncio.
 */
function marcarCaminhoDoCinema(ligada) {
  const imagem = $("palco-imagem");
  if (!imagem) return;
  for (let no = imagem; no && no !== document.body; no = no.parentElement) {
    const pai = no.parentElement;
    if (!pai) break;
    for (const irmao of pai.children) {
      if (irmao === no) continue;
      if (ligada) irmao.dataset.foraDoCinema = "sim";
      else delete irmao.dataset.foraDoCinema;
    }
    // O próprio caminho estica; a imagem não, que é quem recebe o tamanho.
    if (no !== imagem) {
      if (ligada) no.dataset.noCinema = "sim";
      else delete no.dataset.noCinema;
    }
  }
}

/** Entra ou sai da tela cheia, nos dois lados. */
async function trocarCinema(ligada) {
  noCinema = ligada;
  if (ligada) {
    document.body.dataset.cinema = "sim";
  } else {
    delete document.body.dataset.cinema;
  }
  marcarCaminhoDoCinema(ligada);
  const botao = $("palco-cheia");
  if (botao) botao.textContent = ligada ? "SAIR DA TELA CHEIA" : "TELA CHEIA";
  try {
    await invoke("tela_cheia", { ligada });
  } catch (falha) {
    console.warn("tela_cheia:", falha);
  }
}

/**
 * Mostra ou esconde o botão, conforme haja imagem.
 *
 * Chamado de `desenharPalco`: é ele que sabe se há transmissão, e duplicar essa
 * decisão aqui daria duas respostas para a mesma pergunta.
 */
function botaoDeCinema(temImagem) {
  const botao = $("palco-cheia");
  if (!botao) return;
  botao.hidden = !temImagem;
  // A transmissão acabou enquanto alguém assistia em tela cheia: sair sozinho,
  // porque o que sobra é uma janela preta sem nada dizendo como voltar.
  if (!temImagem && noCinema) {
    trocarCinema(false).catch((falha) => console.warn("sair do cinema:", falha));
  }
}

$("palco-cheia").addEventListener("click", () => {
  trocarCinema(!noCinema).catch((falha) => console.warn("cinema:", falha));
});

// Duplo clique na imagem, que é o gesto que todo mundo já tem no dedo.
$("palco-imagem").addEventListener("dblclick", () => {
  trocarCinema(!noCinema).catch((falha) => console.warn("cinema:", falha));
});

// `Escape` sai, e **antes** de qualquer outro ouvinte de `Escape` desta janela:
// em tela cheia não há nenhuma outra coisa aberta para a tecla fechar, e a
// alternativa era a pessoa apertar Esc e ver uma busca fechar atrás de uma
// imagem que continua cobrindo tudo.
window.addEventListener(
  "keydown",
  (evento) => {
    if (evento.key !== "Escape" || !noCinema) return;
    evento.preventDefault();
    evento.stopPropagation();
    trocarCinema(false).catch((falha) => console.warn("sair do cinema:", falha));
  },
  true,
);

listen("seele://event", (evento) => {
  const carga = evento.payload;
  if (!carga || typeof carga !== "object") return;
  if (carga.ScreenOpened) {
    const { screen, width, height } = carga.ScreenOpened;
    abrirImagemDaTela(screen, width, height).catch((falha) =>
      console.warn("abrir a imagem da tela:", falha),
    );
  } else if (carga.ScreenFrame) {
    const { screen, key, data } = carga.ScreenFrame;
    quadroDaTela(screen, key, data);
  } else if (carga.ScreenClosed) {
    if (carga.ScreenClosed.screen === telaEmCurso) fecharImagemDaTela();
  }
});
