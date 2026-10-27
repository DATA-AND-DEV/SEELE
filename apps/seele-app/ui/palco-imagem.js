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
 * O que esta pessoa **pediu** para ver, que não é o mesmo que está vendo.
 *
 * # Por que a intenção precisa de uma variável própria
 *
 * `telaEmCurso` é escrito quando o fluxo **abre de verdade**, e abrir espera o
 * próximo quadro-chave de quem transmite — segundos, não milissegundos. Enquanto
 * a fileira era desenhada só a partir dele, o intervalo inteiro entre o clique e
 * a imagem era indistinguível de nada ter acontecido: o botão continuava dizendo
 * `VER`, sem marca, e clicar de novo **reiniciava a espera**.
 *
 * Foi relatado assim: «o botão de ver e não ver a stream ta completamente
 * cagado. Tem hora que clico e ele não vai, as vezes cliquei pra não ver ele
 * fecha e depois abre novamente sozinho.» As duas metades saem daqui.
 *
 * # Os três estados, e eles são três de propósito
 *
 * - `undefined` — **ninguém escolheu**. O servidor decide, e ele liga todo mundo
 *   na primeira transmissão da sala. É o certo para quem não disse nada: ninguém
 *   quer clicar para ver a única coisa que está acontecendo.
 * - `null` — **escolheu não ver**. Vale contra o religar do servidor.
 * - um número — **escolheu esta**, e a imagem pode ainda não ter chegado.
 *
 * A ausência de escolha e a escolha de não ver eram a mesma coisa antes, com uma
 * bandeira à parte tentando distingui-las. Duas variáveis para um estado são
 * duas variáveis que discordam.
 */
let telaQuerida;

/**
 * Em que sala a escolha acima vale.
 *
 * **Presa à sala, e não zerada por qualquer retrato.** A bandeira anterior era
 * apagada sempre que um retrato não afirmasse que se está numa sala — e um
 * retrato transitório, chegando entre dois estados, afirma isso sem querer. A
 * escolha morria, o servidor religava, e a imagem voltava sozinha.
 *
 * Comparar a sala é o que distingue «saí» de «o retrato ainda não sabe».
 */
let salaDaEscolha = null;

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
 * **A época do palco**: sobe a cada abertura, fechamento e troca de sessão
 * (`fecharImagemDaTela`). O número da transmissão NÃO identifica uma abertura:
 * ele se repete entre sessões e reconexões (a transmissão 5 de um servidor, e
 * a 5 de novo depois de reconectar). Uma promessa que voltou (`isConfigSupported`)
 * ou um decodificador que fala (`output`, `error`) só toca no palco se a época
 * dele ainda for a de pé — e a geração da sessão também (revisão do Codex).
 */
let epocaDoPalco = 0;

/** A identidade de uma operação do palco: a época e a geração da sessão. */
function identidadeDoPalco() {
  return { epoca: epocaDoPalco, geracao: typeof geracaoDaSessao === "number" ? geracaoDaSessao : 0 };
}

/** A operação ainda é a de pé: a mesma abertura, na mesma sessão. */
function aindaDoPalco(identidade) {
  return identidade.epoca === epocaDoPalco &&
    identidade.geracao === (typeof geracaoDaSessao === "number" ? geracaoDaSessao : 0);
}

/**
 * **O pedido de quadro-chave para RECUPERAR** (item 1 da revisão de
 * comunicação e telas; Codex). O codificador só manda quadro-chave no começo e
 * quando alguém pede. A abertura pede um; mas um decodificador que morre no
 * meio do fluxo (`error`), ou deltas chegando sem decodificador armado, ficavam
 * esperando a chave que ninguém pedia — a imagem não voltava enquanto os
 * deltas corriam. Agora a perda pede de novo, com limite:
 *
 * - nada de pedido novo enquanto um está pendente e dentro do intervalo;
 * - o intervalo dobra a cada pedido sem chave (1 s, 2 s, 4 s… até 16 s);
 * - a chave que chega zera tudo (`chaveChegou`).
 *
 * `relogioDoPalco` é a régua do tempo (a bancada a troca).
 */
let relogioDoPalco = () => performance.now();
const CHAVE_PRIMEIRO_INTERVALO_MS = 1000;
const CHAVE_INTERVALO_MAXIMO_MS = 16000;
const pedidoDeChave = { pendenteDesde: null, intervalo: CHAVE_PRIMEIRO_INTERVALO_MS };

/** Anota um pedido de chave saindo agora (a abertura, ou a recuperação). */
function marcarPedidoDeChave() {
  pedidoDeChave.pendenteDesde = relogioDoPalco();
}

/** A chave chegou: a recuperação acabou, e o intervalo volta ao começo. */
function chaveChegou() {
  pedidoDeChave.pendenteDesde = null;
  pedidoDeChave.intervalo = CHAVE_PRIMEIRO_INTERVALO_MS;
}

/**
 * Pede o quadro-chave de novo, se o limite deixa: a imagem se perdeu (o
 * decodificador morreu, ou deltas chegam sem ele) e só uma chave a traz.
 */
function recuperarAChave() {
  const tela = telaEmCurso;
  if (tela === null) return;
  const agora = relogioDoPalco();
  if (pedidoDeChave.pendenteDesde !== null) {
    if (agora - pedidoDeChave.pendenteDesde < pedidoDeChave.intervalo) return;
    // O pedido anterior não trouxe chave a tempo: o próximo espera o dobro.
    pedidoDeChave.intervalo = Math.min(pedidoDeChave.intervalo * 2, CHAVE_INTERVALO_MAXIMO_MS);
  }
  pedidoDeChave.pendenteDesde = agora;
  const minha = identidadeDoPalco();
  invoke("pedir_quadro_chave", { tela }).catch((falha) => {
    if (!aindaDoPalco(minha)) return;
    if (falha !== "NotConnected") console.warn("pedir_quadro_chave (recuperar):", falha);
  });
}

/** As medidas que a transmissão anunciou, guardadas até o SPS chegar. */
let medidasDaTela = null;

/** Se `armarPeloSps` está em curso: `isConfigSupported` é assíncrona. */
let armando = false;

/**
 * Quantos quadros chegaram e foram descartados enquanto `armando`. Os deltas que
 * vêm DEPOIS referenciam o que se perdeu, e o `VideoDecoder` não dá erro: pinta
 * uma imagem errada (medido no WebKit: 21 quadros descartados, a imagem final a
 * 11,33 da origem contra 2,79 sem a demora, sem erro e sem pedido de chave).
 */
let soltadosNoArmando = 0;

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
  // **A recusa vale mesmo quando o servidor liga por conta própria.**
  //
  // Ele liga todo mundo na primeira transmissão de uma sala, sem pedido, e isso
  // é o certo para quem não disse nada — ninguém quer clicar para ver a única
  // coisa que está acontecendo. Para quem disse, é o contrário: a cópia chega
  // por um caminho que a pessoa já recusou, e devolvê-la é a única forma de o
  // botão significar o que diz.
  // Escolheu não ver, ou escolheu **outra**: nos dois casos o servidor está
  // abrindo uma coisa que ninguém pediu, e devolvê-la é o que faz a escolha
  // significar alguma coisa. `undefined` passa: aí ninguém escolheu, e o
  // religar automático é o comportamento certo.
  if (telaQuerida !== undefined && telaQuerida !== tela) {
    invoke("assistir", { tela, quero: false }).catch((falha) => {
      if (falha !== "NotConnected") console.warn("recusar a transmissão:", falha);
    });
    return;
  }
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

  // **E um quadro-chave é pedido.**
  //
  // Um fluxo H.264 é um quadro-chave e uma corrente de diferenças que só fazem
  // sentido a partir dele. O codificador manda um no começo e depois **só
  // quando alguém pede** — é medida, não descuido: um quadro-chave de 1080p
  // custa quatro vezes um comum.
  //
  // Quem estava na sala quando a transmissão começou recebe o fluxo desde o
  // primeiro byte, e o primeiro byte é aquele quadro-chave. Quem **entra
  // depois** pega a corrente no meio: só diferenças, de um quadro que nunca
  // viu, que o decodificador descarta uma a uma. A tela ficava vazia até a
  // transmissão acabar e recomeçar — «quando alguém entra numa call que alguém
  // tá compartilhando tela, a pessoa não consegue ver a transmissão».
  //
  // O pedido existia dos dois lados desde sempre, e ninguém o fazia. Feito aqui
  // e não depois do primeiro quadro porque é aqui que se sabe que há uma
  // transmissão: esperar por um quadro-chave para pedir um quadro-chave é a
  // espera que nunca termina.
  marcarPedidoDeChave();
  invoke("pedir_quadro_chave", { tela }).catch((falha) => {
    // Sem sessão não há a quem pedir, e é o caso do espelho de quem compartilha
    // — a imagem dele não passa pelo servidor.
    if (falha !== "NotConnected") console.warn("pedir_quadro_chave:", falha);
  });
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
  soltadosNoArmando = 0;
  const daTela = telaEmCurso;
  // A identidade desta armação: a época do palco e a geração da sessão, e não
  // só o número da transmissão, que se repete entre sessões.
  const minha = identidadeDoPalco();
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
    // A transmissão, a abertura ou a sessão podem ter trocado enquanto se
    // esperava — e a transmissão nova pode ter o MESMO número.
    if (daTela !== telaEmCurso || !aindaDoPalco(minha)) return;
    if (!veredito.supported) {
      console.warn("esta janela não decodifica", config.codec);
      // O código do perfil junto: é ele que diz a quem conserta **qual**
      // formato esta máquina recusou, e sem ele a frase não ajuda ninguém.
      naoDeuParaMostrar(`ESTA MÁQUINA NÃO DECODIFICA ${config.codec}`);
      return;
    }

    // Os dois retornos do decodificador conferem que ele ainda é o do palco:
    // um decodificador de uma abertura que já fechou não pinta nem apaga nada.
    const este = new VideoDecoder({
      output: (quadro) => {
        if (decodificador !== este || !aindaDoPalco(minha)) {
          quadro.close();
          return;
        }
        pintar(quadro);
      },
      error: (falha) => {
        console.warn("decodificador de tela:", falha);
        if (decodificador !== este || !aindaDoPalco(minha)) return;
        naoDeuParaMostrar("O DECODIFICADOR DE VÍDEO PAROU · ESPERANDO O PRÓXIMO QUADRO-CHAVE");
        // Morreu: o próximo quadro-chave arma outro. Não adianta insistir com
        // este — um `VideoDecoder` em erro não volta.
        decodificador = null;
        esperandoChave = true;
        // E o próximo quadro-chave é PEDIDO: sem pedido, ele não vem, e a
        // imagem ficava parada com os deltas correndo (`recuperarAChave`).
        recuperarAChave();
      },
    });
    decodificador = este;
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
    // **O que chegou enquanto se armava foi descartado**, e os deltas que vêm
    // agora referenciam o que se perdeu: o decodificador pintaria uma imagem
    // errada, sem erro. A chave que armou foi entregue (é uma imagem inteira, e
    // jogá-la fora deixou a tela preta no 0.8.x), mas os deltas que vêm agora
    // esperam uma chave NOVA: é um estado de recuperação, com o pedido e o
    // backoff de sempre (`recuperarAChave`: cada delta depois do prazo repete o
    // pedido, o intervalo dobra até o teto). A chave nova sai da recuperação
    // (`entregarAoDecodificador`), e os deltas voltam a ser aceitos. Sem perda
    // nada disto roda: a chave entra e os deltas seguem.
    if (soltadosNoArmando > 0) {
      soltadosNoArmando = 0;
      esperandoChave = true;
      recuperarAChave();
    }
  } catch (falha) {
    console.warn("armar o decodificador:", falha);
    // A falha de uma armação velha não fala por cima da abertura nova.
    if (aindaDoPalco(minha)) naoDeuParaMostrar("NÃO CONSEGUI PREPARAR O VÍDEO NESTA MÁQUINA");
  } finally {
    // Só a armação de pé solta a marca: a velha que termina depois não
    // destrava a nova no meio da espera dela.
    if (aindaDoPalco(minha)) armando = false;
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
    // Armando: o quadro é descartado, e fica contado — a chave que armou não é
    // mais a cabeça deste fluxo (ver `armarPeloSps`).
    if (armando) {
      soltadosNoArmando += 1;
      return;
    }
    if (chave) armarPeloSps(bytes);
    // Deltas sem decodificador: a sincronia se perdeu (ou a chave da abertura
    // não chegou). Um pedido, com limite — ver `recuperarAChave`.
    else recuperarAChave();
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
    if (!chave) {
      recuperarAChave();
      return;
    }
    esperandoChave = false;
  }
  // A chave entregue encerra a recuperação.
  if (chave) chaveChegou();

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
  soltarVideoDaTelaCheia();
  // Toda abertura, todo fechamento e toda troca de sessão passam aqui: o que
  // ainda estiver em voo de antes deixa de valer (`epocaDoPalco`).
  epocaDoPalco += 1;
  // O pedido de chave em curso era da abertura que fecha.
  chaveChegou();
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
  soltadosNoArmando = 0;
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

/**
 * **O palco é da sessão, e sai com ela** (a frente «a tela desaparece depois
 * de reconectar o servidor inteiro», comunicacao-telas-escopo.md).
 *
 * Tudo o que este arquivo guarda — o decodificador, a tela em curso, a
 * escolha (`telaQuerida`) e a sala dela — fala dos números que UM servidor deu
 * numa sessão. Nada disso era esquecido ao sair: desconectar o servidor
 * inteiro e voltar deixava a escolha de antes de pé, presa a uma sala de mesmo
 * número, e o clique em VER na transmissão de sempre caía no «já pedida»
 * (`trocarDeTransmissao`) — nenhum `assistir` saía, e a assinatura da sessão
 * nova (que o Rust guarda por sessão) nunca era feita. A imagem tinha sumido
 * e não havia clique que a trouxesse.
 *
 * **Encerrar não é ocultar** (revisão do Codex). Esta é a de ENCERRAR —
 * `esquecerOEstadoDaSessao` (tela-sessao.js: SAIR, RECONECTAR, a volta à
 * entrada) —, e ela apaga a escolha. A troca de vista entre sessões, em que a
 * sessão continua conectada e a assinatura dela continua no Rust, é
 * `ocultarOPalco`/`restaurarOPalco`, abaixo: a escolha fica guardada com a
 * sessão e volta com ela. Nenhuma das três manda nada ao servidor.
 */
function esquecerOPalco() {
  // A abertura registrada da sessão que ENCERRA sai com ela — mas não a que
  // `ocultarOPalco` acabou de guardar: a troca de vista passa pelas duas.
  const vista = typeof vistaDaJanela === "object" ? vistaDaJanela : null;
  if (vista && vista.sessao !== null && ocultadaAgora !== chaveDoPalco(vista.sessao, vista.geracao)) {
    esquecerAberturas(vista.sessao);
  }
  ocultadaAgora = null;
  novaIntencao();
  fecharImagemDaTela();
  telaQuerida = undefined;
  salaDaEscolha = null;
  if (noCinema) trocarCinema(false).catch((falha) => console.warn("sair do cinema:", falha));
}

/**
 * A escolha do palco de cada sessão que saiu da vista, pela chave
 * `sessão#geração`: uma reconexão é uma geração nova, e nunca herda a escolha
 * guardada da anterior.
 */
const palcoPorSessao = new Map();

function chaveDoPalco(sessao, geracao) {
  return `${sessao}#${geracao ?? 0}`;
}

/**
 * **A sessão sai da vista, e continua**: o decodificador e a imagem saem (são
 * desta tela, e os quadros da sessão do fundo não chegam a ela), e a escolha
 * — VER esta, NÃO VER, ninguém escolheu — é guardada com a sessão.
 */
function ocultarOPalco(sessao, geracao) {
  if (sessao === null || sessao === undefined) {
    esquecerOPalco();
    return;
  }
  palcoPorSessao.set(chaveDoPalco(sessao, geracao), { telaQuerida, salaDaEscolha });
  // A abertura em curso vai ao registro da sessão: é dela que a volta religa,
  // se nada mais novo chegar enquanto ela está fora (`palcoDeFora`).
  if (telaEmCurso !== null && medidasDaTela) {
    registrarAbertura(sessao, geracao, { tela: telaEmCurso, largura: medidasDaTela.largura, altura: medidasDaTela.altura });
  } else {
    esquecerAberturas(sessao);
  }
  ocultadaAgora = chaveDoPalco(sessao, geracao);
  novaIntencao();
  fecharImagemDaTela();
  telaQuerida = undefined;
  salaDaEscolha = null;
  if (noCinema) trocarCinema(false).catch((falha) => console.warn("sair do cinema:", falha));
}

// ------------------------------------------- o registro das sessões de fora
//
// **A tela que a sessão de fora assiste não pode se perder** (achado do CUA na
// 0.17: A → outra sessão → A, e a imagem sumia com a legenda ativa e o seletor
// em NÃO VER). O `listen` do base.js desvia os eventos de uma sessão fora da
// vista — o palco é da vista —, e com eles iam o `ScreenOpened` de uma leitura
// nova e o `ScreenClosed` da que acabou. E a volta não tinha o que religar: o
// servidor não reabre para quem já assiste, e o Rust não reemite a abertura.
//
// O registro guarda, por `sessão#geração`, a abertura vigente da sessão de
// fora: a que estava no palco quando ela saiu, trocada pelo `ScreenOpened` que
// chegar enquanto ela está fora, apagada pelo `ScreenClosed` da mesma tela. Os
// eventos já vêm filtrados pela leitura vigente na FFI (`Shared::tela_fechou`,
// `quadro_da_tela`, `som_da_leitura_vale`): o registro guarda a abertura que o
// palco teria aberto se a sessão estivesse na vista, e nada mais.

/** A abertura vigente de cada sessão de fora, por `sessão#geração`. */
const aberturaPorSessao = new Map();

/**
 * O teto do registro. Uma entrada por sessão (a geração nova substitui a
 * velha), e as sessões que fecham saem por `podarOPalco`; o teto é a parede
 * para o que escapar das duas.
 */
const TETO_DO_REGISTRO = 16;

/** A chave que `ocultarOPalco` acabou de guardar: o `esquecerOPalco` da mesma troca não a apaga. */
let ocultadaAgora = null;

function registrarAbertura(sessao, geracao, abertura) {
  const chave = chaveDoPalco(sessao, geracao);
  esquecerAberturas(sessao);
  aberturaPorSessao.set(chave, abertura);
  while (aberturaPorSessao.size > TETO_DO_REGISTRO) {
    aberturaPorSessao.delete(aberturaPorSessao.keys().next().value);
  }
}

function esquecerAberturas(sessao) {
  for (const chave of [...aberturaPorSessao.keys()]) {
    if (chave.startsWith(`${sessao}#`)) aberturaPorSessao.delete(chave);
  }
}

/**
 * Um evento de tela de uma sessão FORA da vista, antes de o base.js o desviar.
 * Só anota: nada é decodificado nem pedido para uma sessão que não está na tela.
 */
function palcoDeFora(sessao, geracao, evento) {
  if (!evento || typeof evento !== "object") return;
  if (evento.ScreenOpened) {
    const { screen, width, height } = evento.ScreenOpened;
    registrarAbertura(sessao, geracao, { tela: screen, largura: width, altura: height });
  } else if (evento.ScreenClosed) {
    const chave = chaveDoPalco(sessao, geracao);
    if (aberturaPorSessao.get(chave)?.tela === evento.ScreenClosed.screen) aberturaPorSessao.delete(chave);
  }
}

/** A transmissão alheia que o retrato desenhado diz que esta sessão assiste. */
function telaAssistidaNoRetrato() {
  const retrato = typeof desenhado !== "undefined" ? desenhado : null;
  const tela = retrato?.tela;
  return tela && !tela.e_minha && typeof tela.tela === "number" ? tela.tela : null;
}

/**
 * A sessão volta à vista: a escolha dela volta, se foi guardada NESTA geração,
 * e a imagem volta pela abertura registrada — se o retrato da volta (o Rust)
 * ainda diz que é essa a tela assistida.
 *
 * Chamada depois de `desenhar(entrada.snapshot)` (`verSessao`), então o
 * `desenhado` é o retrato da volta.
 */
function restaurarOPalco(sessao, geracao) {
  const chave = chaveDoPalco(sessao, geracao);
  const guardado = palcoPorSessao.get(chave);
  palcoPorSessao.delete(chave);
  // As de outras gerações da mesma sessão são de antes de uma reconexão:
  // não valem mais, e saem.
  for (const outra of [...palcoPorSessao.keys()]) {
    if (outra.startsWith(`${sessao}#`)) palcoPorSessao.delete(outra);
  }
  const abertura = aberturaPorSessao.get(chave) ?? null;
  esquecerAberturas(sessao);
  if (guardado) {
    telaQuerida = guardado.telaQuerida;
    salaDaEscolha = guardado.salaDaEscolha;
  }
  // NÃO VER explícito: nada abre, e nada sai.
  if (telaQuerida === null) return;
  // O Rust é quem diz o que esta sessão assiste agora. Sem tela assistida (a
  // transmissão acabou, ou a da geração velha morreu com ela), o palco desenha
  // «ninguém está compartilhando», e não há o que religar.
  const assistida = telaAssistidaNoRetrato();
  if (assistida === null) return;
  // Escolheu outra, que ainda não abriu: a fileira diz ABRINDO, e a abertura
  // dela chega pelo caminho normal.
  if (typeof telaQuerida === "number" && telaQuerida !== assistida) return;
  if (abertura && abertura.tela === assistida) {
    abrirImagemDaTela(abertura.tela, abertura.largura, abertura.altura).catch((falha) =>
      console.warn("religar a imagem da tela:", falha),
    );
    return;
  }
  // O Rust diz que há tela assistida e a janela não tem a abertura dela.
  religarPeloRetrato(assistida).catch((falha) => console.warn("religar pelo retrato:", falha));
}

/**
 * **A saída quando há tela assistida e nenhuma abertura** para ela: larga e
 * pede de novo, com o número que o RETRATO dá — e não o `telaEmCurso`, que é
 * `null` justamente aqui. Pedir só de novo não basta: o servidor ignora quem
 * já assiste (`voice_room.rs`, `assistir`), e nenhuma abertura viria. Largar e
 * pedir dá uma leitura nova, e o `ScreenOpened` dela abre o palco pelo caminho
 * normal (`abrirImagemDaTela`), com os filtros de leitura vigente da FFI.
 */
async function religarPeloRetrato(tela) {
  const intencao = novaIntencao();
  telaQuerida = tela;
  const sessao = vistaDaJanela.sessao;
  const geracao = vistaDaJanela.geracao;
  naoDeuParaMostrar("A IMAGEM DESTA TRANSMISSÃO NÃO ESTAVA CHEGANDO · REABRINDO");
  await invoke("assistir", { tela, quero: false }).catch((falha) => {
    if (falha !== "NotConnected") console.warn("religar (largar):", falha);
  });
  // Um NÃO VER, outra escolha ou outra vista enquanto se largava: esta
  // função não tem mais nada a dizer.
  if (intencao !== geracaoDaIntencao || sessao !== vistaDaJanela.sessao || geracao !== vistaDaJanela.geracao) return;
  await invoke("assistir", { tela, quero: true });
}

/** Tira as escolhas guardadas de sessões (ou gerações) que já não estão abertas. */
function podarOPalco(carimbos) {
  const abertas = new Set((carimbos ?? []).filter(Boolean).map((c) => chaveDoPalco(c.sessao, c.geracao)));
  for (const chave of [...palcoPorSessao.keys()]) if (!abertas.has(chave)) palcoPorSessao.delete(chave);
  for (const chave of [...aberturaPorSessao.keys()]) if (!abertas.has(chave)) aberturaPorSessao.delete(chave);
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

// O compartilhamento é decodificado no canvas. Só quando a pessoa pede tela
// cheia, e o navegador oferece apenas fullscreen de vídeo, nasce esta cópia.
// Ela não captura microfone/áudio nem altera a transmissão recebida.
let videoDaTelaCheia = null;
let vezDoCinema = 0;

function soltarVideoDaTelaCheia() {
  const video = videoDaTelaCheia;
  if (!video) return;
  videoDaTelaCheia = null;
  clearTimeout(video.prazoDaTelaCheia);
  try { if (video.webkitDisplayingFullscreen) video.webkitExitFullscreen(); } catch { /* já saiu */ }
  for (const track of video.srcObject?.getTracks() ?? []) track.stop();
  video.pause();
  video.srcObject = null;
  video.remove();
}

function prepararVideoDaTelaCheia() {
  if (videoDaTelaCheia) return videoDaTelaCheia;
  const canvas = $("palco-imagem");
  const video = document.createElement("video");
  if (!video.webkitEnterFullscreen || !canvas?.captureStream || !canvas.width || !canvas.height) return null;
  video.id = "palco-video-nativo";
  video.muted = true;
  video.playsInline = true;
  video.controls = true;
  video.setAttribute("aria-hidden", "true");
  Object.assign(video.style, { position: "fixed", width: "1px", height: "1px", bottom: "0", left: "0", opacity: "0.001", pointerEvents: "none" });
  try { video.srcObject = canvas.captureStream(30); } catch { return null; }
  videoDaTelaCheia = video;
  document.body.append(video);
  video.addEventListener("webkitendfullscreen", () => {
    if (videoDaTelaCheia === video && noCinema) void trocarCinema(false);
  });
  video.addEventListener("loadedmetadata", () => {
    clearTimeout(video.prazoDaTelaCheia);
    if (videoDaTelaCheia !== video || !noCinema) return;
    document.body.dataset.cinemaModo = video.webkitSupportsFullscreen === false ? "pagina" : "video-pronto";
    if (video.webkitSupportsFullscreen === false) soltarVideoDaTelaCheia();
  });
  video.prazoDaTelaCheia = setTimeout(() => {
    if (videoDaTelaCheia !== video || video.readyState >= 1) return;
    document.body.dataset.cinemaModo = "pagina";
    soltarVideoDaTelaCheia();
  }, 3000);
  video.play().catch(() => {
    if (videoDaTelaCheia !== video) return;
    document.body.dataset.cinemaModo = "pagina";
    soltarVideoDaTelaCheia();
  });
  return video;
}

/** Resultado honesto: CSS ampliado não é fullscreen do navegador. */
async function trocarTelaCheiaDoNavegador(ligada) {
  const doc = document;
  const dentro = () => doc.fullscreenElement || doc.webkitFullscreenElement;
  if (!ligada) {
    soltarVideoDaTelaCheia();
    if (dentro()) await (doc.exitFullscreen?.() ?? doc.webkitExitFullscreen?.());
    return { modo: "fora" };
  }
  if (dentro()) return { modo: "nativa" };
  const raiz = doc.documentElement;
  const pedir = raiz.requestFullscreen ?? raiz.webkitRequestFullscreen;
  let genericaRecusou = false;
  if (pedir && !videoDaTelaCheia && doc.fullscreenEnabled !== false && doc.webkitFullscreenEnabled !== false) {
    try {
      await pedir.call(raiz);
      if (dentro()) return { modo: "nativa" };
      genericaRecusou = true;
    } catch { genericaRecusou = true; }
  }
  const video = prepararVideoDaTelaCheia();
  if (!video) return { modo: "pagina" };
  if (video.readyState < 1) return { modo: "preparando-video" };
  if (video.webkitSupportsFullscreen === false) { soltarVideoDaTelaCheia(); return { modo: "pagina" }; }
  // Uma recusa assíncrona pode consumir o gesto. Oferece outro, já preparado.
  if (genericaRecusou) return { modo: "video-pronto" };
  return new Promise((resolve) => {
    let timer;
    const terminar = () => {
      clearTimeout(timer);
      video.removeEventListener("webkitbeginfullscreen", terminar);
      const nativa = videoDaTelaCheia === video && video.webkitDisplayingFullscreen;
      if (!nativa) soltarVideoDaTelaCheia();
      resolve({ modo: nativa ? "nativa" : "pagina" });
    };
    video.addEventListener("webkitbeginfullscreen", terminar);
    timer = setTimeout(terminar, 1500);
    // O player nativo conserva propriedades do elemento: não deve herdar
    // transparência/tamanho do vídeo usado apenas para preparar metadata.
    video.removeAttribute("aria-hidden");
    Object.assign(video.style, { width: "100%", height: "100%", opacity: "1", objectFit: "contain", background: "black", pointerEvents: "auto" });
    try { video.webkitEnterFullscreen(); } catch { terminar(); }
  });
}

for (const evento of ["fullscreenchange", "webkitfullscreenchange"]) {
  document.addEventListener(evento, () => {
    if (noCinema
        && !document.fullscreenElement && !document.webkitFullscreenElement) void trocarCinema(false);
  });
}

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
  const minha = ++vezDoCinema;
  const mudou = noCinema !== ligada;
  noCinema = ligada;
  if (ligada) {
    document.body.dataset.cinema = "sim";
  } else {
    delete document.body.dataset.cinema;
    delete document.body.dataset.cinemaModo;
  }
  if (mudou) marcarCaminhoDoCinema(ligada);
  const botao = $("palco-cheia");
  if (botao) botao.textContent = ligada ? "SAIR DA TELA CHEIA" : "TELA CHEIA";
  try {
    const resultado = await invoke("tela_cheia", { ligada });
    if (minha !== vezDoCinema) {
      if (!noCinema && ligada) await invoke("tela_cheia", { ligada: false });
      return;
    }
    if (ligada && resultado?.modo) {
      // Metadata pode chegar enquanto a chamada volta à casca.
      const modo = resultado.modo === "preparando-video" && videoDaTelaCheia?.readyState >= 1
        ? "video-pronto" : resultado.modo;
      document.body.dataset.cinemaModo = modo;
      if (modo !== "nativa" && botao) botao.textContent = FRASES_DO_CELULAR.cinema.voltar;
      if (modo === "pagina" && typeof anunciar === "function") anunciar(FRASES_DO_CELULAR.cinema.indisponivel);
    }
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

// A barra compacta do palco pede para parar de assistir, e quem sabe parar é
// este arquivo — é ele que guarda a intenção e o decodificador. Um evento e não
// uma chamada direta porque os dois scripts não se importam: `tela-chamada.js`
// desenha, este conduz.
window.addEventListener("seele:parar-de-assistir", () => {
  pararDeVer().catch((falha) => console.warn("parar de assistir:", falha));
});

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
  } else if (carga.ScreenUnreadable) {
    // **A frase que faltava.**
    //
    // Chegou uma transmissão e este build não soube ler o cabeçalho dela — quase
    // sempre porque o outro lado está noutra versão. Antes disto o núcleo
    // voltava calado: sem `ScreenOpened` não havia o que desenhar, sem
    // `ScreenClosed` não havia o que apagar, e a pessoa ficava olhando um
    // retângulo escuro. Relatado assim: «vê tela preta, sem mensagem nenhuma».
    //
    // O motivo técnico vai para o console, e não para a tela: quem está numa
    // call não tem o que fazer com «unsupported version 3, expected 2». O que
    // ela pode fazer está na frase.
    console.warn("transmissão ilegível:", carga.ScreenUnreadable.reason);
    naoDeuParaMostrar(
      "ALGUÉM ESTÁ COMPARTILHANDO NUMA VERSÃO QUE ESTA NÃO LÊ · ATUALIZE O SEELE",
    );
  }
});

// ---------------------------------------------------------------- a escolha

/**
 * Desenha a lista de transmissões da sala e marca a que está no palco.
 *
 * # Por que ela mora aqui
 *
 * Porque quem sabe **qual está sendo recebida** é este arquivo: `telaEmCurso` é
 * escrito quando um fluxo abre de verdade, e não quando o servidor anuncia que
 * alguém começou. As duas coisas são diferentes desde que a segunda transmissão
 * de uma sala passou a esperar um pedido — o anúncio chega para todo mundo, a
 * imagem só para quem pediu.
 *
 * # Aparece com uma, e não só com duas
 *
 * Ela ficava escondida com uma transmissão só, com o argumento de que escolher
 * entre uma coisa não é escolher. O argumento estava incompleto: com uma
 * transmissão há **duas** escolhas — ver, e não ver —, e a segunda não tinha por
 * onde ser feita. Pedido assim: «precisamos da opção do usuário estar na call e
 * não querer ver a live.»
 *
 * Quem não quer ver tem razões que este produto não precisa conhecer: a máquina
 * dele decodifica 1080p com o ventilador no teto, a banda dele é dividida com
 * mais gente, ou ele só quer ouvir. O que custava a ele era uma cópia na
 * descida e um decodificador na CPU que ninguém tinha pedido.
 *
 * @param {object} snapshot o retrato que a sessão acabou de ler
 */
function desenharTransmissoes(snapshot) {
  const onde = $("palco-escolha");
  if (!onde) return;

  // **A escolha morre com a sala, e só com ela.**
  //
  // Ela era apagada sempre que um retrato não afirmasse que se está numa sala —
  // e um retrato transitório, chegando entre dois estados, afirma isso sem
  // querer. A escolha morria, o servidor religava, e a imagem voltava sozinha:
  // «cliquei pra não ver, ele fecha e depois abre novamente sozinho».
  //
  // Duas mudanças resolvem. A primeira: só um retrato que **traz a lista de
  // salas** conta — sem ela não se sabe nada, e não saber não é motivo para
  // esquecer. A segunda: compara-se a sala, e não a presença; entrar noutra sala
  // é uma escolha nova, ficar na mesma não é.
  if (Array.isArray(snapshot?.voice_rooms)) {
    const sala = snapshot.voice_rooms.find((uma) => uma.occupied_by_us)?.id ?? null;
    if (sala !== salaDaEscolha) {
      salaDaEscolha = sala;
      telaQuerida = undefined;
    }
  }

  const lista = Array.isArray(snapshot?.transmissoes) ? snapshot.transmissoes : [];
  // Quem transmite não assiste a si mesmo pelo servidor — o espelho dele é
  // local —, então a própria transmissão não entra na escolha.
  const alheias = lista.filter((transmissao) => !transmissao.e_minha);
  onde.hidden = alheias.length === 0;
  if (onde.hidden) {
    onde.replaceChildren();
    return;
  }

  // **O que está marcado é o que foi pedido, e não o que já chegou.**
  //
  // Sem escolha nenhuma, o pedido é o que está no palco — é o servidor quem
  // decide, e a fileira mostra o que ele decidiu.
  const pedida = telaQuerida === undefined ? telaEmCurso : telaQuerida;

  const botoes = alheias.map((transmissao) => {
    const quem = nomeDeQuem(snapshot, transmissao.de);
    const escolhida = transmissao.tela === pedida;
    const noPalco = transmissao.tela === telaEmCurso;
    // **Esperando é um terceiro estado, e ele precisava aparecer.**
    //
    // Entre o clique e a imagem há o próximo quadro-chave de quem transmite —
    // segundos. A fileira mostrava o fluxo, então o intervalo inteiro era
    // indistinguível de nada ter acontecido: «tem hora que clico e ele não vai».
    // Agora ela mostra o pedido, e diz que está esperando.
    const esperando = escolhida && !noPalco;

    // **`VER` na frente quando não se está vendo nada.**
    //
    // O rótulo era só o nome. Entre duas transmissões isso basta — a escolha é
    // entre pessoas, e a marcada diz qual está no palco. Mas depois de apertar
    // `NÃO VER` não há nenhuma marcada, e a fileira vira uma lista de nomes: um
    // nome não se parece com um caminho de volta.
    //
    // Relatado assim: «tem botão pra parar de ver live, mas não tem botão pra
    // voltar a ver a live». O botão existia e não dizia que era um.
    const rotulo = esperando
      ? `${quem} · ABRINDO`
      : pedida === null
        ? `VER ${quem}`
        : quem;
    const botao = elemento("button", null, rotulo);
    botao.type = "button";
    botao.setAttribute("aria-pressed", escolhida ? "true" : "false");
    botao.dataset.esperando = esperando ? "sim" : "nao";
    botao.title = esperando
      ? `pedido; a imagem começa no próximo quadro-chave de ${quem}`
      : noPalco
        ? `${quem} está no palco`
        : `ver a tela de ${quem}`;
    botao.addEventListener("click", () => {
      trocarDeTransmissao(transmissao.tela).catch((falha) =>
        console.warn("trocar de transmissão:", falha),
      );
    });
    return botao;
  });

  // **E a saída, no fim da fileira.**
  //
  // No fim e não no começo: a fileira é lida da esquerda, e o que se procura
  // primeiro é a tela de alguém. Um `NÃO VER` na frente poria a recusa antes da
  // oferta.
  //
  // Marcado como qualquer um dos outros — `aria-pressed` — porque ele é uma
  // opção da mesma escolha, e não uma ação à parte: estar sem ver nenhuma é um
  // dos estados possíveis, e quem chega tem de conseguir dizer em qual está.
  const nenhuma = elemento("button", null, "NÃO VER");
  nenhuma.type = "button";
  nenhuma.setAttribute("aria-pressed", pedida === null ? "true" : "false");
  nenhuma.title =
    pedida === null
      ? "você não está recebendo transmissão nenhuma"
      : "parar de receber a transmissão e ficar só no áudio";
  nenhuma.addEventListener("click", () => {
    pararDeVer().catch((falha) => console.warn("parar de ver:", falha));
  });
  botoes.push(nenhuma);

  repovoar(onde, botoes);
}

/**
 * Quantas vezes a intenção mudou nesta janela.
 *
 * # O que ela conserta
 *
 * R16 da revisão da v15: «"não ver" precisa cancelar também as operações
 * pendentes».
 *
 * `pararDeVer` guardava a recusa e fechava a imagem; `trocarDeTransmissao`
 * esperava o cancelamento da anterior e **depois** pedia a nova, sem revalidar a
 * intenção. Reproduzido: assistir A → pedir B → clicar «não ver» enquanto o
 * cancelamento de A estava pendente → concluir esse cancelamento. A escolha
 * ficava `null` e a janela mandava `assistir(B, true)` **depois** do clique.
 *
 * Toda operação assíncrona daqui guarda esta geração antes do primeiro `await` e
 * a confere depois de cada um. Uma que não bate é obsoleta, e obsoleta quer
 * dizer descartada — não «aplicada mesmo assim porque já chegou até aqui».
 */
let geracaoDaIntencao = 0;

/** Anda a geração e devolve a nova. Todo gesto que muda a intenção passa aqui. */
function novaIntencao() {
  geracaoDaIntencao += 1;
  return geracaoDaIntencao;
}

/**
 * Larga a transmissão que está no palco, e não pega outra.
 *
 * A escolha fica **guardada no Rust** desde o ADR 0054 — `assistir` a grava
 * antes de o pedido sair —, e é o que faz o botão significar o que ele diz. Sem
 * isso, a próxima transmissão a começar sozinha na sala seria empurrada de volta
 * pelo servidor, que liga todo mundo na primeira; é o comportamento certo para
 * quem não disse nada. Quem disse, disse.
 *
 * A cópia local continua aqui porque ela é lida **entre** um pedido e a
 * resposta dele, quando o Rust ainda não sabe; as duas andam juntas e a do Rust
 * é a que sobrevive à janela.
 *
 * # Três coisas param, e não uma
 *
 * A imagem, a assinatura e **o cinema**. A terceira era o R19: `pararDeVer`
 * fechava a imagem e não saía da tela cheia, e `botaoDeCinema` só sai quando não
 * há transmissão no palco — mas o desenho continuava passando `true`, porque a
 * outra pessoa continuava transmitindo. O resultado era navegação escondida por
 * CSS com nada na frente dela.
 */
async function pararDeVer() {
  const intencao = novaIntencao();
  // **A que se larga é a que o Rust assina**, e não só a que está no palco.
  // Com o palco vazio, ninguém tendo escolhido e o retrato com tela (o estado
  // medido no CUA), o NÃO VER não mandava nada: a assinatura continuava e o
  // clique não fazia o que diz. Com uma escolha em curso (`telaQuerida`
  // numérica), quem larga é a própria troca (`trocarDeTransmissao`), e a
  // abertura que chegar depois é recusada por `abrirImagemDaTela`.
  const antiga = telaEmCurso ?? (telaQuerida === undefined ? telaAssistidaNoRetrato() : null);
  telaQuerida = null;
  fecharImagemDaTela();
  // **Sai do cinema antes de esperar qualquer coisa.** Se ficasse depois do
  // `await`, uma recusa lenta deixaria a navegação escondida durante a espera —
  // que é o sintoma relatado, só que mais curto.
  if (noCinema) {
    await trocarCinema(false);
  }
  if (antiga !== null) {
    await invoke("assistir", { tela: antiga, quero: false });
  }
  // A conferência depois do `await`: se alguém escolheu uma transmissão
  // enquanto o cancelamento corria, esta função não tem mais nada a dizer.
  if (intencao !== geracaoDaIntencao) return;
}

/**
 * O nome de quem transmite, ou um travessão se os presentes ainda não o têm.
 *
 * Os presentes (`Snapshot::presentes`, de `seele-ffi`) são a única fonte de nome
 * desta casca. Um id cru na tela seria pior que o travessão: parece um dado e
 * não é nome de ninguém.
 *
 * Isto lia `snapshot.roster`, que o `Snapshot` nunca teve, e a fileira dizia
 * «VER —» para todo mundo, desde a 0.15.
 */
function nomeDeQuem(snapshot, id) {
  const pessoa = (snapshot?.presentes ?? []).find((quem) => quem.id === id);
  return apelidoApresentado(id, pessoa?.nickname ?? "—", snapshot?.identidades_apresentadas ?? {});
}

/**
 * Troca o que está no palco.
 *
 * **Larga a antiga antes de pedir a nova**, e a ordem é a conta: as duas ao
 * mesmo tempo seriam duas cópias saindo da subida de quem hospeda por um
 * instante — e é justamente o instante em que o teto seria recalculado, podendo
 * derrubar as duas.
 */
async function trocarDeTransmissao(tela) {
  // **Comparado com o que foi pedido, e não com o que está na tela.**
  //
  // Era `tela === telaEmCurso`, e entre o clique e a imagem esse teste é falso —
  // o fluxo ainda não abriu. Então um segundo clique refazia o pedido inteiro e
  // **reiniciava a espera pelo quadro-chave**, que é o que fazia o botão parecer
  // morto quanto mais se insistia nele.
  if (tela === telaQuerida) return;
  const intencao = novaIntencao();
  telaQuerida = tela;
  const antiga = telaEmCurso;
  // A imagem some agora, e não quando a nova chegar: deixar a antiga desenhada
  // enquanto a nova não vem faria a troca parecer que não aconteceu.
  fecharImagemDaTela();
  if (antiga !== null) {
    await invoke("assistir", { tela: antiga, quero: false }).catch((falha) =>
      console.warn("parar de assistir:", falha),
    );
  }
  // **A intenção é revalidada depois de cada espera.** Era aqui que o R16
  // mordia: o cancelamento da anterior é assíncrono, e um clique em «não ver»
  // durante ele deixava sair o pedido abaixo mesmo assim — um pedido obsoleto
  // reativando a assinatura que alguém acabou de recusar.
  if (intencao !== geracaoDaIntencao) {
    return;
  }
  await invoke("assistir", { tela, quero: true });
}
