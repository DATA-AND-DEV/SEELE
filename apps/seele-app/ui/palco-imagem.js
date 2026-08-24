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

/**
 * O nível do H.264 que cabe uma imagem desta altura.
 *
 * O perfil é sempre baseline — `crates/seele-video/src/codec.rs` escolhe CAVLC
 * justamente para o OpenH264 não subir para High —, então o que varia é o
 * nível, e ele tem de caber a resolução ou o decodificador recusa a configuração
 * inteira. Os números são os da tabela A-1 do H.264.
 */
function nivelDoCodec(altura) {
  if (altura <= 480) return "1e"; // 3.0
  if (altura <= 720) return "1f"; // 3.1
  if (altura <= 1080) return "28"; // 4.0
  return "33"; // 5.1, para monitores acima de 1080p
}

/** Desenha um quadro decodificado e o solta. */
function pintar(quadro) {
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
    return;
  }

  const config = {
    codec: `avc1.42e0${nivelDoCodec(altura)}`,
    codedWidth: largura,
    codedHeight: altura,
    // Sem `description`: é o que diz ao decodificador que o fluxo é Annex-B,
    // que é como o OpenH264 entrega e como `Transmissao` põe no fio.
    optimizeForLatency: true,
  };

  try {
    const veredito = await VideoDecoder.isConfigSupported(config);
    if (!veredito.supported) {
      console.warn("esta janela não decodifica", config.codec);
      return;
    }
  } catch (falha) {
    console.warn("isConfigSupported:", falha);
    return;
  }

  decodificador = new VideoDecoder({
    output: pintar,
    error: (falha) => {
      console.warn("decodificador de tela:", falha);
      // Morreu: o próximo quadro-chave arma outro. Não adianta insistir com
      // este — um `VideoDecoder` em erro não volta.
      decodificador = null;
      esperandoChave = true;
    },
  });
  decodificador.configure(config);
}

/** Recebe um quadro comprimido em base64 e o entrega ao decodificador. */
function quadroDaTela(tela, chave, base64) {
  if (tela !== telaEmCurso || !decodificador) return;
  if (esperandoChave) {
    if (!chave) return;
    esperandoChave = false;
  }

  let bytes;
  try {
    const cru = atob(base64);
    bytes = new Uint8Array(cru.length);
    for (let i = 0; i < cru.length; i += 1) bytes[i] = cru.charCodeAt(i);
  } catch (falha) {
    console.warn("quadro de tela ilegível:", falha);
    return;
  }

  try {
    decodificador.decode(
      new EncodedVideoChunk({
        type: chave ? "key" : "delta",
        // Um relógio nosso, e monotônico: o protocolo não carrega carimbo de
        // tempo, e o `VideoDecoder` exige um. O valor não é lido por ninguém —
        // não há sincronismo com áudio a fazer aqui —, só precisa crescer.
        timestamp: carimbo,
        data: bytes,
      }),
    );
    carimbo += 33_333;
  } catch (falha) {
    console.warn("decode:", falha);
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
 * dele. A primeira versão da tela cheia usava `#tela-chamada > *:not(#palco)`,
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
