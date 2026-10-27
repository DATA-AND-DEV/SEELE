// A voz no navegador (Tarefa 3.3 da F4, lote C): o microfone, o codec, o
// transporte e o alto-falante, em volta do `.wasm` do `seele-web`.
//
// # Quem faz a conta
//
// **O `.wasm`**, e não este arquivo (ADR 0057: «o JavaScript só move bytes
// opacos»). O portão, o relógio de mídia e o cabeçalho ficam no `voz::Envio`;
// a separação por fonte, no `voz::Recepcao`, que entrega uma **alça** no lugar
// do SSRC; o buffer de jitter e o mixer, no `voz::Reproducao`, que roda no
// `AudioWorklet` (`voz-worklet.js`). Daqui saem o `getUserMedia`, o
// `AudioEncoder`, um `AudioDecoder` por alça, o grafo de áudio e o
// encanamento entre eles.
//
// # A configuração é a da rodada 5
//
// O spike (`spikes/voz-no-navegador/`, rodada 5 no iPhone) manteve voz nos
// dois sentidos com a tela bloqueada numa aba do Safari com a sessão de áudio
// declarada como ligação (`play-and-record`, antes de o `AudioContext` nascer)
// e a saída por uma tag `<audio>`. O ADR 0057 fixa essa configuração inteira,
// e ela está em `CONFIGURACAO_DA_VOZ` e em `ligar`, na ordem do spike.
//
// # O relógio é o da placa de som
//
// Nada daqui depende de temporizador: com a página oculta, o `setInterval`
// cai para um por segundo. A captura anda pelo `process()` do worklet de
// captura, e o esquecimento de quem calou anda pelo relato do worklet de
// reprodução, que é um por segundo **do relógio de áudio**.
//
// # O que para é dito
//
// A faixa do microfone muda ou encerrada, o `AudioContext` fora de `running`,
// a tag de saída pausada, um `messageerror` numa porta, um codec que falhou:
// cada um vira um evento em `aoAcontecer` e uma conta em `contas()`. «Quando a
// captura ou a reprodução para, a pessoa e a sala ficam sabendo» (ADR 0057). A
// sala fica sabendo pela sessão, que é a Tarefa 2.6; daqui sai o evento.
//
// # No desktop, nada
//
// Este arquivo é carregado nas duas cascas e no desktop só declara: a voz de
// lá é a do `seele-core`, e ninguém chama `criarVozDoNavegador` nela.

/** A configuração da voz no navegador. */
const CONFIGURACAO_DA_VOZ = Object.freeze({
  /** 20 ms a 48 kHz, o quadro do Opus no produto (`specs/03-audio.md`). */
  quadro: 960,
  taxa: 48000,
  /**
   * O DSP da plataforma, aceito pelo ADR 0057 (emenda do ADR 0007): o
   * cancelamento de eco, a supressão e o ganho do navegador. A supressão segue
   * a escolha da pessoa, e o portão usa a régua do que **foi aplicado**, lido
   * da faixa.
   */
  microfone: Object.freeze({ echoCancellation: true, autoGainControl: true, channelCount: 1 }),
  /**
   * O codificador do desktop (`seele_audio::codec`): 48 kbps, voz, 20 ms e
   * FEC desligada (ADR 0010).
   *
   * **Sem o DTX, que o desktop liga.** Medido no Chromium em 10/10/2026
   * (`bancada/voz-no-navegador.cjs`): com `usedtx: true`, o `AudioEncoder`
   * segura 17 quadros antes de devolver o primeiro, e a voz sai 340 ms
   * atrasada para sempre; sem ele, o atraso é zero quadro. O que o DTX
   * economiza no fio o portão já economiza: fechado, nada sai.
   */
  encoder: Object.freeze({
    codec: "opus",
    sampleRate: 48000,
    numberOfChannels: 1,
    bitrate: 48000,
    opus: Object.freeze({ frameDuration: 20000, application: "voip", useinbandfec: false }),
  }),
  decoder: Object.freeze({ codec: "opus", sampleRate: 48000, numberOfChannels: 1 }),
  /** O módulo dos dois processadores, ao lado desta página. */
  worklet: "voz-worklet.js",
  /** A sessão de áudio do WebKit da rodada 5: uma ligação. */
  sessaoDeAudio: "play-and-record",
  /**
   * Quanto a captura fica ligada ao grafo antes de ser dada como aberta. O
   * Safari do simulador do iOS 27 encerrou a faixa com o cancelamento de eco
   * 4 ms depois de ela nascer viva (preparacao-0.17, lifecycle-2): 300 ms
   * cobrem o arranque com folga, e é o que a abertura da voz espera a mais.
   */
  arranqueDaCapturaMs: 300,
});

/** O `.wasm` do `seele-web`, ao lado da página. */
const ARQUIVO_DO_WASM_DA_PAGINA = "seele_web.wasm";

let wasmDaPagina = null;

/**
 * O `.wasm` da página e os bytes dele, carregados uma vez, no primeiro uso.
 * Uma carga que falhou não fica guardada: o uso seguinte tenta de novo.
 */
function carregarOWasmDaPagina() {
  if (!wasmDaPagina) {
    wasmDaPagina = fetch(ARQUIVO_DO_WASM_DA_PAGINA)
      .then((resposta) => {
        if (!resposta.ok) throw new Error(`${ARQUIVO_DO_WASM_DA_PAGINA}: ${resposta.status}`);
        return resposta.arrayBuffer();
      })
      .then(async (bytes) => ({ bytes, instancia: (await WebAssembly.instantiate(bytes, {})).instance }));
    wasmDaPagina.catch(() => {
      wasmDaPagina = null;
    });
  }
  return wasmDaPagina;
}

/**
 * A alça do monitor do teste do microfone na mistura. A recepção dá alças a
 * partir de 1, e voltar ao zero pediria quatro bilhões de fontes.
 */
const ALCA_DO_MONITOR = 0;

/**
 * A alça do som da tela na mistura: a última que cabe num `u32`, que a
 * recepção (de 1 em diante, uma por voz) nunca alcança — como o `SSRC_DA_TELA`
 * do desktop.
 */
const ALCA_DA_TELA = 0xffffffff;

/**
 * A voz do navegador, em volta das exportações do `.wasm` do `seele-web`.
 *
 * - `wasm`: as exportações da instância da página (a mesma da sessão, quando
 *   ela existir: a voz e a sala dividem o `Room`);
 * - `bytesDoWasm`: os bytes do mesmo `.wasm`, que o worklet compila lá dentro;
 * - `aoAcontecer(evento)`: cada coisa que a pessoa precisa saber.
 *
 * O transporte é `{ enviar(pacote) → boolean, ouvir(fn) → parar }`: a sessão
 * pelo WebTransport (Tarefa 2.6), ou um laço de bancada.
 */
function criarVozDoNavegador({ wasm, bytesDoWasm, aoAcontecer = () => {} }) {
  const w = wasm;
  const cfg = CONFIGURACAO_DA_VOZ;

  /**
   * **Os controles guardados**, que sobrevivem a uma reconexão: são vestidos
   * de novo a cada `entrar` e `reconectar` (o Step 1 da Tarefa 3.3). Os ganhos
   * são por alça, e uma recepção nova recomeça as alças; o ganho por pessoa
   * chega com a sessão, que liga a pessoa à alça.
   */
  const controles = {
    mudo: false,
    isolado: false,
    /** Os bytes do `VoiceMode` do desktop: 0 aperte-para-falar, 1 por voz, 2 aberto. */
    modo: 0,
    tecla: false,
    /** Milésimos de dBFS; zero é o padrão, que segue a supressão. */
    abertura: 0,
    supressao: true,
    ganhos: new Map(),
  };

  const contas = {
    capturados: 0,
    semEntrada: 0,
    /** Os quadros que passaram pelo portão: os capturados dentro da sala. */
    noPortao: 0,
    codificados: 0,
    semCarimbo: 0,
    enviados: 0,
    recusadosPeloTransporte: 0,
    dtx: 0,
    chegados: 0,
    daPropriaVoz: 0,
    tortos: 0,
    decodificados: 0,
    semCabecalho: 0,
    esquecidas: 0,
    falhas: [],
  };

  /**
   * Quantas vezes o áudio foi fechado. Um `ligar` em voo confere o número que
   * viu ao começar: se mudou, o que chegou depois (o `getUserMedia` que resolve
   * tarde) é de um áudio que já foi fechado, e não é adotado.
   */
  let geracaoDoAudio = 0;

  const vivo = {
    ctx: null,
    /** O stream inteiro do `getUserMedia`: o `desligar` para TODAS as faixas dele. */
    fonte: null,
    /** Se os ouvintes do fim da página estão registrados (um só jogo, sem duplicar). */
    escutandoAPagina: false,
    faixa: null,
    captura: null,
    reproducao: null,
    elemento: null,
    /** A `MediaStreamDestination` da saída: o `desligar` para as faixas do stream dela. */
    destino: null,
    /**
     * O que a página tinha ANTES do `ligar` (`audioSession.type`, `mediaSession.metadata`),
     * guardado uma vez por abertura, para o `desligar` devolver.
     */
    sessao: null,
    encoder: null,
    supressaoAplicada: null,
    decoders: new Map(),
    transporte: null,
    pararDeOuvir: null,
    dentro: false,
    falando: false,
    reproducaoPronta: null,
    /** O teste do microfone (F01), fora ou dentro da sala. */
    testando: false,
    monitorando: false,
    falhaDoTeste: null,
    seqDoMonitor: 0,
  };

  // ------------------------------------------------------------ o .wasm

  function comBytes(bytes, f) {
    const p = w.alocar(bytes.length);
    new Uint8Array(w.memory.buffer).set(bytes, p);
    try {
      return f(p, bytes.length);
    } finally {
      w.liberar(p, bytes.length);
    }
  }
  const saida = () => new Uint8Array(w.memory.buffer, w.saida_ptr(), w.saida_len()).slice();
  const saidaEmTexto = () => new TextDecoder().decode(saida());
  const bytesDe = (f32) => new Uint8Array(f32.buffer, f32.byteOffset, f32.byteLength);

  function falha(onde, texto) {
    const registro = { onde, texto: String(texto) };
    contas.falhas.push(registro);
    if (contas.falhas.length > 50) contas.falhas.shift();
    aoAcontecer({ tipo: "falha", ...registro });
  }

  // ------------------------------------------------------- o que sai

  /**
   * O número de cada `AudioData` → o carimbo do quadro, até o Opus sair.
   *
   * **Pelo número do quadro, e não pelo instante exato.** O instante que entra
   * é o número vezes 20 000 µs, e o que o Chromium devolve às vezes vem um
   * microssegundo antes (259 999 por 260 000, medido no codificador e no
   * decodificador): a conta dele vai e volta de amostras. Casar pelo instante
   * exato perdia um pedaço em cada vinte e poucos.
   */
  const carimbos = new Map();
  let proximoQuadro = 0;
  const numeroDoQuadro = (instante) => Math.round(instante / 20000);

  function criarEncoder() {
    const encoder = new AudioEncoder({
      output: (pedaco) => {
        contas.codificados++;
        const numero = numeroDoQuadro(pedaco.timestamp);
        const carimbo = carimbos.get(numero);
        carimbos.delete(numero);
        if (carimbo === undefined) {
          contas.semCarimbo++;
          return;
        }
        const opus = new Uint8Array(pedaco.byteLength);
        pedaco.copyTo(opus);
        const codigo = comBytes(opus, (p, n) => w.voz_empacotar(carimbo, p, n));
        if (codigo === 2) {
          contas.dtx++;
        } else if (codigo !== 0) {
          falha("envio", saidaEmTexto());
        } else if (vivo.transporte && vivo.transporte.enviar(saida())) {
          contas.enviados++;
        } else {
          contas.recusadosPeloTransporte++;
        }
      },
      error: (erro) => falha("codificador", erro),
    });
    encoder.configure(cfg.encoder);
    return encoder;
  }

  function codificar(carimbo, pcm) {
    const encoder = vivo.encoder;
    if (!encoder || encoder.state !== "configured") return;
    const numero = proximoQuadro++;
    carimbos.set(numero, carimbo);
    // Um codificador que engolisse quadros sem saída deixaria o mapa crescer.
    if (carimbos.size > 200) carimbos.delete(carimbos.keys().next().value);
    const dados = new AudioData({
      format: "f32-planar",
      sampleRate: cfg.taxa,
      numberOfFrames: cfg.quadro,
      numberOfChannels: 1,
      timestamp: numero * 20000,
      data: pcm,
    });
    try {
      encoder.encode(dados);
    } catch (erro) {
      falha("codificador", erro);
    } finally {
      dados.close();
    }
  }

  /**
   * Um quadro de 20 ms para o teste do microfone: o portão do teste, e o que
   * ele soltar vai ao monitor, se ligado, pela mesma mistura da voz.
   */
  function testou(pcm) {
    const quantos = comBytes(bytesDe(pcm), (p, n) => w.voz_teste_quadro(p, n)) >>> 0;
    if (quantos === 0xffff_ffff) {
      falha("teste do microfone", saidaEmTexto());
      return;
    }
    if (!vivo.monitorando || quantos === 0) return;
    const bytes = saida();
    const vista = new DataView(bytes.buffer);
    const passo = 4 + 4 * cfg.quadro;
    for (let i = 0; i < quantos; i++) {
      const quadro = new Float32Array(bytes.buffer.slice(i * passo + 4, (i + 1) * passo));
      vivo.seqDoMonitor = (vivo.seqDoMonitor + 1) & 0xffff;
      mandarAReproducao(
        {
          tipo: "chegou",
          alca: ALCA_DO_MONITOR,
          seq: vivo.seqDoMonitor,
          carimbo: vista.getUint32(i * passo, true),
          chegada: performance.now(),
          pcm: quadro,
        },
        [quadro.buffer],
      );
    }
  }

  /** Um quadro de 20 ms do worklet de captura: o portão, e o que ele soltar. */
  function capturou(pcm) {
    contas.capturados++;
    if (vivo.testando) testou(pcm);
    if (!vivo.dentro) return;
    contas.noPortao++;
    const quantos = comBytes(bytesDe(pcm), (p, n) => w.voz_capturou(p, n, controles.mudo ? 1 : 0)) >>> 0;
    if (quantos === 0xffff_ffff) {
      falha("portão", saidaEmTexto());
      return;
    }
    const falando = quantos > 0;
    if (falando !== vivo.falando) {
      vivo.falando = falando;
      aoAcontecer({ tipo: "falando", falando });
    }
    if (!falando) return;
    const bytes = saida();
    const vista = new DataView(bytes.buffer);
    const passo = 4 + 4 * cfg.quadro;
    for (let i = 0; i < quantos; i++) {
      const carimbo = vista.getUint32(i * passo, true);
      const quadro = new Float32Array(bytes.buffer.slice(i * passo + 4, (i + 1) * passo));
      codificar(carimbo, quadro);
    }
  }

  // ------------------------------------------------------ o que chega

  /** O número de cada pedaço de Opus → o cabeçalho dele, até o som sair. */
  const chegadas = new Map();
  let proximaChegada = 0;

  function decodificado(alca, dados) {
    const numero = numeroDoQuadro(dados.timestamp);
    const info = chegadas.get(numero);
    chegadas.delete(numero);
    const pcm = new Float32Array(dados.numberOfFrames);
    try {
      // Mono: o plano 0 é o som inteiro, em `f32` ou `f32-planar`.
      const formato = dados.format === "f32" || dados.format === "f32-planar" ? {} : { format: "f32-planar" };
      dados.copyTo(pcm, { planeIndex: 0, ...formato });
    } catch (erro) {
      falha(`decodificador da alça ${alca}`, erro);
      return;
    } finally {
      dados.close();
    }
    if (!info) {
      contas.semCabecalho++;
      return;
    }
    contas.decodificados++;
    mandarAReproducao({ tipo: "chegou", alca, seq: info.seq, carimbo: info.carimbo, chegada: info.chegada, pcm }, [pcm.buffer]);
  }

  function decoderDe(alca) {
    let decoder = vivo.decoders.get(alca);
    if (!decoder) {
      decoder = new AudioDecoder({
        output: (dados) => decodificado(alca, dados),
        error: (erro) => falha(`decodificador da alça ${alca}`, erro),
      });
      decoder.configure(cfg.decoder);
      vivo.decoders.set(alca, decoder);
    }
    return decoder;
  }

  /** Um pacote do transporte: bytes opacos, que só o `.wasm` lê. */
  function chegou(pacote) {
    const agora = performance.now();
    const codigo = comBytes(pacote, (p, n) => w.voz_chegou(p, n, agora));
    if (codigo === 2) {
      contas.daPropriaVoz++;
      return;
    }
    if (codigo !== 0) {
      contas.tortos++;
      return;
    }
    contas.chegados++;
    const bytes = saida();
    const vista = new DataView(bytes.buffer);
    const alca = vista.getUint32(0, true);
    const numero = proximaChegada++;
    chegadas.set(numero, { seq: vista.getUint16(4, true), carimbo: vista.getUint32(6, true), chegada: agora });
    if (chegadas.size > 500) chegadas.delete(chegadas.keys().next().value);
    try {
      decoderDe(alca).decode(new EncodedAudioChunk({ type: "key", timestamp: numero * 20000, data: bytes.subarray(10) }));
    } catch (erro) {
      falha(`decodificador da alça ${alca}`, erro);
    }
  }

  // ------------------------------------------- o som da tela (R17, ADR 0054)

  /**
   * **O som da transmissão que se assiste**, como o do desktop: uma tela por
   * vez (trocar de tela joga fora o que sobrou da anterior), numa fonte própria
   * da mistura, com o volume e o mudo dela — que não mexem na voz de ninguém.
   * Os quadros de som chegam pelo fluxo da tela, sem número nem carimbo: a
   * ordem de chegada é a de saída, e cada um é um quadro de 20 ms.
   */
  const somDaTela = { tela: null, seq: 0, carimbo: 0, volume: 1, calada: false, quadros: 0 };

  function aplicarOGanhoDaTela() {
    mandarAReproducao({ tipo: "ganho", alca: ALCA_DA_TELA, ganho: somDaTela.calada ? 0 : somDaTela.volume });
  }

  function tocarATela(tela, opus) {
    if (!vivo.reproducao || !vivo.dentro) return;
    if (somDaTela.tela !== tela) {
      esquecerOSomDaTela();
      somDaTela.tela = tela;
      aplicarOGanhoDaTela();
    }
    const numero = proximaChegada++;
    chegadas.set(numero, { seq: somDaTela.seq, carimbo: somDaTela.carimbo, chegada: performance.now() });
    if (chegadas.size > 500) chegadas.delete(chegadas.keys().next().value);
    somDaTela.seq = (somDaTela.seq + 1) & 0xffff;
    somDaTela.carimbo = (somDaTela.carimbo + cfg.quadro) >>> 0;
    somDaTela.quadros += 1;
    try {
      decoderDe(ALCA_DA_TELA).decode(new EncodedAudioChunk({ type: "key", timestamp: numero * 20000, data: opus }));
    } catch (erro) {
      falha("decodificador do som da tela", erro);
    }
  }

  /** Esquece o som da tela: o de `tela`, ou, sem ela, o que estiver tocando. */
  function esquecerOSomDaTela(tela) {
    if (tela !== undefined && somDaTela.tela !== tela) return;
    if (somDaTela.tela === null) return;
    fecharDecoder(ALCA_DA_TELA);
    mandarAReproducao({ tipo: "esquecer", alca: ALCA_DA_TELA });
    Object.assign(somDaTela, { tela: null, seq: 0, carimbo: 0 });
  }

  /** Esquece quem calou: o `.wasm` diz quem, e o decodificador de cada um fecha. */
  function esquecerCaladas() {
    if (!vivo.dentro) return;
    const quantas = w.voz_esquecer(performance.now());
    if (quantas === 0) return;
    const vista = new DataView(saida().buffer);
    for (let i = 0; i < quantas; i++) {
      const alca = vista.getUint32(i * 4, true);
      fecharDecoder(alca);
      mandarAReproducao({ tipo: "esquecer", alca });
      controles.ganhos.delete(alca);
      contas.esquecidas++;
    }
  }

  function fecharDecoder(alca) {
    const decoder = vivo.decoders.get(alca);
    vivo.decoders.delete(alca);
    if (decoder && decoder.state !== "closed") decoder.close();
  }

  // --------------------------------------------------------- o worklet

  function mandarAReproducao(mensagem, transferir) {
    if (vivo.reproducao) vivo.reproducao.port.postMessage(mensagem, transferir ?? []);
  }

  const pedidosDeContas = new Map();
  let proximoPedido = 0;

  function ouvirAReproducao(no) {
    // Quanto o `.wasm` leva para compilar dentro do worklet, medido daqui: o
    // escopo do worklet não tem relógio de parede, e o de áudio pode nem ter
    // começado a andar.
    const nasceu = performance.now();
    let pronta;
    vivo.reproducaoPronta = new Promise((resolve) => {
      pronta = resolve;
    });
    no.port.onmessage = (evento) => {
      const m = evento.data;
      if (m.tipo === "pronto") {
        aoAcontecer({ tipo: "reproducao-pronta", compilouEmMs: performance.now() - nasceu });
        pronta(true);
      } else if (m.tipo === "andamento") {
        esquecerCaladas();
      } else if (m.tipo === "contas") {
        const resolver = pedidosDeContas.get(m.pedido);
        pedidosDeContas.delete(m.pedido);
        if (resolver) resolver(m);
      } else if (m.tipo === "falha") {
        falha(m.onde, m.texto);
        if (m.texto.startsWith("o .wasm") || m.texto.startsWith("o worklet nasceu")) pronta(false);
      }
    };
    // **O `messageerror` que o spike achou calado**: o Chromium recusa assim um
    // `WebAssembly.Module` mandado pela porta, e sem este ouvinte a recusa
    // não aparece em lugar nenhum.
    no.port.onmessageerror = () => falha("reprodução", "uma mensagem do worklet não atravessou a porta (messageerror)");
  }

  // ----------------------------------------------------- os controles

  /**
   * Veste o `.wasm` e o worklet com os controles guardados. Chamado a cada
   * `entrar` e `reconectar`: um portão novo ou uma mistura recomeçada não
   * podem esquecer o que a pessoa escolheu.
   */
  function vestir() {
    aplicarRegua();
    w.voz_modo(controles.modo);
    w.voz_tecla(controles.tecla ? 1 : 0);
    mandarAReproducao({ tipo: "isolado", isolado: controles.isolado });
    for (const [alca, ganho] of controles.ganhos) mandarAReproducao({ tipo: "ganho", alca, ganho });
  }

  function aplicarRegua() {
    // A régua é a do que **foi aplicado**: um navegador que recusou a supressão
    // deixa o portão como única defesa, e o padrão volta aos −42 dBFS.
    const comSupressao = vivo.supressaoAplicada ?? controles.supressao;
    w.voz_regua(controles.abertura, comSupressao ? 1 : 0);
  }

  // ------------------------------------------------- abrir e fechar

  /**
   * Abre o áudio. **Dentro do gesto**: o `AudioContext` e o `play()` da saída
   * pedem um toque, e no iPhone a sessão de áudio tem de ser declarada antes
   * do contexto nascer. A ordem é a do spike.
   *
   * `microfone` e `saida` são os aparelhos pedidos; o que abriu é lido de volta
   * e dito em `contas()`.
   */
  async function ligar(pedido = {}) {
    if (vivo.ctx) return;
    const minhaGeracao = geracaoDoAudio;
    try {
      await abrirOAudio(pedido, minhaGeracao);
    } catch (erro) {
      // Uma abertura pela metade (o microfone recusado, a saída que não
      // tocou) não deixa contexto nem tag de áudio para trás. **Só a própria
      // geração**: se o áudio já foi fechado (o `pagehide`, o `desligar`), quem
      // fechou já levou o que esta abertura tinha registrado, e uma abertura
      // NOVA pode estar de pé, que este `catch` não pode derrubar.
      if (minhaGeracao === geracaoDoAudio) await desligar();
      throw erro;
    }
  }

  /**
   * **A página saindo solta o microfone.** O `pagehide` (a navegação, o fechar
   * do PWA, o bfcache) e o `freeze` fecham o áudio: `track.stop()` em todas as
   * faixas e o contexto fechado. **Só esses dois**: o `visibilitychange` (a tela
   * bloqueada, outro app) não para a voz, que segue com a tela apagada, e o
   * `pageshow` não religa nada: abrir o microfone pede um gesto, e quem volta
   * entra na sala de novo.
   */
  const aoSairDaPagina = () => {
    desligar().catch((erro) => falha("desligar", erro));
    // Dito a quem mostra o estado da voz (a ponte): o áudio está fechado, e só um
    // gesto da pessoa o reabre.
    aoAcontecer({ tipo: "audio-fechado", motivo: "pagina" });
  };

  function escutarAPagina() {
    if (vivo.escutandoAPagina) return;
    vivo.escutandoAPagina = true;
    window.addEventListener("pagehide", aoSairDaPagina);
    window.addEventListener("freeze", aoSairDaPagina);
    document.addEventListener("freeze", aoSairDaPagina);
  }

  function pararDeEscutarAPagina() {
    if (!vivo.escutandoAPagina) return;
    vivo.escutandoAPagina = false;
    window.removeEventListener("pagehide", aoSairDaPagina);
    window.removeEventListener("freeze", aoSairDaPagina);
    document.removeEventListener("freeze", aoSairDaPagina);
  }

  /**
   * Se a faixa passa do arranque: `true` se ela segue viva depois de `ms`,
   * `false` se encerrar antes. O `onended` de antes volta ao lugar ao fim.
   */
  function sobreviveAoArranque(faixa, ms) {
    return new Promise((resolver) => {
      if (faixa.readyState === "ended") return resolver(false);
      const anterior = faixa.onended;
      let decidido = false;
      const decidir = (subiu) => {
        if (decidido) return;
        decidido = true;
        faixa.onended = anterior;
        resolver(subiu && faixa.readyState !== "ended");
      };
      faixa.onended = () => decidir(false);
      setTimeout(() => decidir(true), ms);
    });
  }

  /** Se o stream do `getUserMedia` chegou sem faixa de áudio viva. */
  function chegouEncerrada(fonte) {
    const faixa = typeof fonte.getAudioTracks === "function" ? fonte.getAudioTracks()[0] : null;
    return !faixa || faixa.readyState === "ended";
  }

  /** Para todas as faixas de um stream do `getUserMedia`. */
  function pararAsFaixas(fonte) {
    const todas = typeof fonte.getTracks === "function" ? fonte.getTracks() : typeof fonte.getAudioTracks === "function" ? fonte.getAudioTracks() : [];
    for (const faixa of todas) faixa.stop();
  }

  async function abrirOAudio({ microfone = null, saida: aparelhoDeSaida = null }, minhaGeracao) {
    // **Depois de cada espera, a geração**: um `addModule`, um `play`, um
    // `resume` ou um `getUserMedia` que resolve depois do cancelamento não abre
    // nada obsoleto (o microfone, o elemento, o nó).
    const conferir = () => {
      if (minhaGeracao !== geracaoDoAudio) throw new Error("o áudio foi fechado antes de abrir");
    };
    escutarAPagina();
    if (!vivo.sessao) vivo.sessao = { tipo: undefined, tipoMudou: false, metadata: null, metadataMudou: false };
    if (navigator.audioSession) {
      if (!vivo.sessao.tipoMudou) {
        vivo.sessao.tipo = navigator.audioSession.type;
        vivo.sessao.tipoMudou = true;
      }
      navigator.audioSession.type = cfg.sessaoDeAudio;
    }
    const ctx = new AudioContext({ sampleRate: cfg.taxa, latencyHint: "interactive" });
    vivo.ctx = ctx;
    ctx.onstatechange = () => aoAcontecer({ tipo: "contexto", estado: ctx.state });

    await ctx.audioWorklet.addModule(cfg.worklet);
    conferir();
    const reproducao = new AudioWorkletNode(ctx, "seele-reproducao-da-voz", {
      numberOfInputs: 0,
      outputChannelCount: [2],
      processorOptions: { wasm: bytesDoWasm.slice(0) },
    });
    vivo.reproducao = reproducao;
    ouvirAReproducao(reproducao);

    // A saída pela tag `<audio>`: o iOS a trata como mídia, que segue com a
    // tela bloqueada.
    const destino = ctx.createMediaStreamDestination();
    vivo.destino = destino;
    reproducao.connect(destino);
    const elemento = document.createElement("audio");
    elemento.playsInline = true;
    elemento.srcObject = destino.stream;
    elemento.onpause = () => aoAcontecer({ tipo: "saida", estado: "pausada" });
    elemento.onplay = () => aoAcontecer({ tipo: "saida", estado: "tocando" });
    document.body.append(elemento);
    vivo.elemento = elemento;
    if (aparelhoDeSaida && elemento.setSinkId) {
      await elemento.setSinkId(aparelhoDeSaida).catch((erro) => falha("saída", erro));
      conferir();
    }
    await elemento.play();
    conferir();
    if (navigator.mediaSession && window.MediaMetadata) {
      if (vivo.sessao && !vivo.sessao.metadataMudou) {
        vivo.sessao.metadata = navigator.mediaSession.metadata;
        vivo.sessao.metadataMudou = true;
      }
      navigator.mediaSession.metadata = new MediaMetadata({ title: "SEELE" });
    }
    await ctx.resume();
    conferir();

    const pedido = {
      ...cfg.microfone,
      noiseSuppression: controles.supressao,
      ...(microfone ? { deviceId: { ideal: microfone } } : {}),
    };
    // O áudio foi fechado enquanto o navegador pensava (o `pagehide`, o `desligar`):
    // as faixas que acabaram de chegar são paradas, e não adotadas.
    const pedirOMicrofone = async (restricoes) => {
      const chegou = await navigator.mediaDevices.getUserMedia({ audio: restricoes });
      if (minhaGeracao !== geracaoDoAudio) {
        pararAsFaixas(chegou);
        throw new Error("o áudio foi fechado antes de o microfone abrir");
      }
      return chegou;
    };
    const captura = new AudioWorkletNode(ctx, "seele-captura-da-voz", { numberOfOutputs: 1 });
    vivo.captura = captura;
    captura.port.onmessage = (evento) => {
      const m = evento.data;
      if (m.tipo === "quadro") {
        contas.semEntrada = m.semEntrada;
        capturou(m.pcm);
      } else if (m.tipo === "falha") {
        falha(m.onde, m.texto);
      }
    };
    captura.port.onmessageerror = () => falha("captura", "uma mensagem do worklet não atravessou a porta (messageerror)");
    // Um nó sem saída ligada pode não ser processado; o ganho zero o mantém
    // puxado sem devolver o microfone ao alto-falante.
    const calado = ctx.createGain();
    calado.gain.value = 0;
    captura.connect(calado).connect(ctx.destination);

    // **A faixa que não sobe** (o Safari do simulador do iOS 27 com o
    // cancelamento de eco; medido em preparacao-0.17): ou o `getUserMedia` já a
    // entrega encerrada (A/B cru), ou ela nasce viva e encerra milissegundos
    // depois, no arranque do processamento de voz (lifecycle-2: 4 ms). Adotá-la
    // seria dizer «áudio ligado» com ninguém ouvindo. Por isso a captura só é
    // dada como aberta depois de ARRANQUE_DA_CAPTURA_MS ligada ao grafo; se
    // encerrar antes, UMA tentativa sem o cancelamento de eco (o resto do pedido
    // igual), e dita. Uma recusa (o `getUserMedia` que rejeita) nunca é
    // repetida, e uma faixa que passou do arranque nunca é trocada: depois do
    // `audio-ligado`, encerrar é a captura perdida, e nada reabre sozinho.
    const tentar = async (restricoes) => {
      const chegou = await pedirOMicrofone(restricoes);
      if (chegouEncerrada(chegou)) {
        pararAsFaixas(chegou);
        return null;
      }
      // Adotada já no arranque: um `desligar` ou um `pagehide` agora a para.
      vivo.fonte = chegou;
      vivo.faixa = chegou.getAudioTracks()[0];
      const origem = ctx.createMediaStreamSource(chegou);
      origem.connect(captura);
      const subiu = await sobreviveAoArranque(vivo.faixa, cfg.arranqueDaCapturaMs);
      conferir();
      if (subiu) return chegou;
      if (typeof origem.disconnect === "function") origem.disconnect();
      pararAsFaixas(chegou);
      vivo.fonte = null;
      vivo.faixa = null;
      return null;
    };
    let semEcoPorque = null;
    let fonte = await tentar(pedido);
    if (!fonte && pedido.echoCancellation !== false) {
      falha("microfone", "a faixa com o cancelamento de eco não subiu (encerrada ao chegar ou no arranque): uma tentativa sem ele");
      fonte = await tentar({ ...pedido, echoCancellation: false });
      semEcoPorque = "a faixa com o cancelamento de eco não subiu";
    }
    if (!fonte) {
      const erro = new Error(
        semEcoPorque ? "o microfone não subiu, com e sem o cancelamento de eco" : "o microfone não subiu",
      );
      erro.name = "SeeleCapturaEncerrada";
      throw erro;
    }
    const faixa = vivo.faixa;
    const aplicado = faixa.getSettings ? faixa.getSettings() : {};
    vivo.supressaoAplicada = typeof aplicado.noiseSuppression === "boolean" ? aplicado.noiseSuppression : null;
    faixa.onmute = () => aoAcontecer({ tipo: "captura", estado: "muda" });
    faixa.onunmute = () => aoAcontecer({ tipo: "captura", estado: "voltou" });
    faixa.onended = () => {
      // **Só a faixa de AGORA**, antes de tudo: a de um áudio já fechado (ou o
      // nosso `stop()`, que o `desligar` faz depois de soltá-la) não escreve o
      // teste, nem o estado, nem fecha o áudio de depois.
      if (faixa !== vivo.faixa) return;
      if (vivo.testando) {
        vivo.falhaDoTeste =
          "o microfone parou durante o teste: o aparelho saiu, ou o navegador o tomou de volta. Tente de novo ou escolha outro microfone.";
      }
      aoAcontecer({ tipo: "captura", estado: "encerrada" });
      // **A captura que parou sozinha, em uso** (o navegador ou o sistema). O
      // áudio fecha inteiro, como no `pagehide`, e a ponte deixa de dizer que a
      // voz está ligada. Nada reabre sozinho: só um gesto.
      falha("microfone", "a captura parou sozinha: o navegador ou o sistema encerrou a faixa");
      desligar().catch((erro) => falha("desligar", erro));
      aoAcontecer({ tipo: "audio-fechado", motivo: "captura-perdida" });
    };

    vivo.encoder = criarEncoderSeguro();
    aoAcontecer({
      tipo: "audio-ligado",
      microfone: aplicado.deviceId ?? null,
      supressao: vivo.supressaoAplicada,
      cancelamentoDeEco: aplicado.echoCancellation ?? null,
      semEcoPorque,
    });
  }

  function criarEncoderSeguro() {
    try {
      return criarEncoder();
    } catch (erro) {
      falha("codificador", erro);
      return null;
    }
  }

  function ouvir(transporte) {
    if (vivo.pararDeOuvir) vivo.pararDeOuvir();
    vivo.transporte = transporte;
    vivo.pararDeOuvir = transporte.ouvir(chegou);
  }

  function fecharDecoders() {
    for (const alca of [...vivo.decoders.keys()]) fecharDecoder(alca);
    chegadas.clear();
  }

  /**
   * Entra na voz da sala. `numeroDaVoz` é o número que o servidor deu à voz
   * desta conexão: vem da sessão e volta ao `.wasm`, e daqui ninguém sabe o
   * que ele é.
   */
  function entrar(transporte, numeroDaVoz) {
    // A voz nova parte da hora da página, e não do zero: quem sai sentado e
    // volta recebe o mesmo número, e do zero ficava mudo para quem ficou
    // (`Envio::partindo_de`). `timeOrigin + now()`, e não `Date.now()`: é
    // monotônico na página, e um acerto do relógio não a faz partir atrás.
    w.voz_envio(numeroDaVoz, performance.timeOrigin + performance.now());
    w.voz_recepcao(numeroDaVoz);
    fecharDecoders();
    mandarAReproducao({ tipo: "recomecar" });
    controles.ganhos.clear();
    vestir();
    ouvir(transporte);
    vivo.dentro = true;
  }

  /**
   * A mesma voz por outra conexão: o relógio salta dois quadros
   * (`voz_reabrir`), a recepção recomeça as alças, e **os controles guardados
   * são vestidos de novo**.
   */
  function reconectar(transporte, numeroDaVoz) {
    w.voz_reabrir(numeroDaVoz);
    w.voz_recepcao(numeroDaVoz);
    fecharDecoders();
    mandarAReproducao({ tipo: "recomecar" });
    controles.ganhos.clear();
    vestir();
    ouvir(transporte);
    vivo.dentro = true;
  }

  /** Sai da voz da sala, com o áudio ainda aberto. */
  function sair() {
    vivo.dentro = false;
    if (vivo.pararDeOuvir) vivo.pararDeOuvir();
    vivo.pararDeOuvir = null;
    vivo.transporte = null;
    fecharDecoders();
    mandarAReproducao({ tipo: "recomecar" });
  }

  /** Fecha o áudio: solta o microfone (`track.stop()`), o codec e o contexto. */
  async function desligar() {
    geracaoDoAudio += 1;
    pararDeEscutarAPagina();
    sair();
    // A faixa de agora deixa de ser «a de agora» ANTES do `stop()`: um navegador
    // que dispare `ended` no nosso próprio `stop()` não conta isso como a
    // captura perdida (o `onended` só age na faixa de agora).
    const faixa = vivo.faixa;
    vivo.faixa = null;
    if (vivo.fonte) pararAsFaixas(vivo.fonte);
    if (faixa) faixa.stop();
    if (vivo.encoder && vivo.encoder.state !== "closed") vivo.encoder.close();
    // A SAÍDA: o `<audio>` pausado, sem o stream e fora do DOM; as faixas da
    // `MediaStreamDestination` paradas. Os ganchos saem antes, para o `pause()`
    // não contar «saída pausada» a quem já não escuta.
    const elemento = vivo.elemento;
    if (elemento) {
      elemento.onpause = null;
      elemento.onplay = null;
      if (typeof elemento.pause === "function") elemento.pause();
      elemento.srcObject = null;
      elemento.remove();
    }
    if (vivo.destino && vivo.destino.stream) pararAsFaixas(vivo.destino.stream);
    // A SESSÃO volta ao que a página tinha antes do `ligar`. **Tudo isto antes
    // do `await`**: depois dele uma abertura NOVA pode já ter posto a sessão
    // dela, e devolver o antigo por cima seria desfazê-la. Uma geração velha
    // não passa por aqui (o `catch` do `abrirOAudio` só fecha a própria), e a
    // nova guarda o seu «antes» do zero.
    const antes = vivo.sessao;
    if (antes) {
      if (antes.tipoMudou && navigator.audioSession) navigator.audioSession.type = antes.tipo;
      if (antes.metadataMudou && navigator.mediaSession) navigator.mediaSession.metadata = antes.metadata;
    }
    const ctx = vivo.ctx;
    Object.assign(vivo, { ctx: null, fonte: null, faixa: null, captura: null, reproducao: null, elemento: null, destino: null, sessao: null, encoder: null });
    if (ctx) await ctx.close();
  }

  /**
   * Abre o teste do microfone (F01): o áudio, se ainda não está aberto, e o
   * portão do teste com a régua dos controles. **Dentro do gesto**, como o
   * `ligar`. Pedir de novo fora da sala fecha o anterior e abre outro, que é o
   * que trocar de aparelho no meio do teste precisa, como no desktop.
   */
  async function abrirTeste({ microfone = null, saida: aparelhoDeSaida = null, supressao = true, abertura = 0 } = {}) {
    if (vivo.testando && !vivo.dentro) await fecharTeste();
    if (!vivo.ctx) {
      controles.supressao = supressao;
      await ligar({ microfone, saida: aparelhoDeSaida });
    }
    const comSupressao = vivo.supressaoAplicada ?? supressao;
    w.voz_teste_abrir(abertura, comSupressao ? 1 : 0);
    Object.assign(vivo, { testando: true, monitorando: false, falhaDoTeste: null });
  }

  /**
   * Fecha o teste. Fora da sala, **solta o microfone de verdade**: as faixas
   * param (`track.stop()`) e o navegador apaga o indicador. Idempotente.
   */
  async function fecharTeste() {
    if (!vivo.testando) return;
    Object.assign(vivo, { testando: false, monitorando: false });
    w.voz_teste_fechar();
    mandarAReproducao({ tipo: "esquecer", alca: ALCA_DO_MONITOR });
    if (!vivo.dentro) await desligar();
  }

  /** O que o teste tem a contar, na forma do desktop, ou `null` sem teste. */
  function estadoDoTeste() {
    if (!vivo.testando) return null;
    if (w.voz_teste_estado() !== 0) {
      falha("teste do microfone", saidaEmTexto());
      return null;
    }
    return { ...JSON.parse(saidaEmTexto()), monitorando: vivo.monitorando, falha: vivo.falhaDoTeste };
  }

  /** As contas desta página, do portão e da mistura. */
  async function lerContas() {
    let portao = null;
    if (vivo.dentro && w.voz_portao_contas() === 0) portao = JSON.parse(saidaEmTexto());
    let reproducao = null;
    if (vivo.reproducao) {
      const pedido = ++proximoPedido;
      reproducao = await new Promise((resolve) => {
        pedidosDeContas.set(pedido, resolve);
        mandarAReproducao({ tipo: "contas", pedido });
      });
    }
    return {
      pagina: {
        ...contas,
        falhas: [...contas.falhas],
        decodificadoresAbertos: vivo.decoders.size,
        /** Quadros dentro do codificador agora: o atraso dele, em quadros. */
        noCodificador: carimbos.size,
      },
      portao,
      reproducao,
      controles: { ...controles, ganhos: Object.fromEntries(controles.ganhos) },
      contexto: vivo.ctx ? vivo.ctx.state : null,
      faixa: vivo.faixa ? { pronta: vivo.faixa.readyState, muda: vivo.faixa.muted } : null,
    };
  }

  return {
    ligar,
    entrar,
    reconectar,
    sair,
    desligar,
    contas: lerContas,
    /** A promessa do `.wasm` compilado no worklet (`false` se falhou). */
    reproducaoPronta: () => vivo.reproducaoPronta,
    abrirTeste,
    fecharTeste,
    estadoDoTeste,
    /** Ouvir o próprio microfone no teste. Começa desligado: realimenta. */
    monitorar(ligado) {
      vivo.monitorando = Boolean(ligado) && vivo.testando;
    },
    mudo(sim) {
      controles.mudo = Boolean(sim);
    },
    isolar(sim) {
      controles.isolado = Boolean(sim);
      mandarAReproducao({ tipo: "isolado", isolado: controles.isolado });
    },
    modo(byte) {
      controles.modo = byte;
      if (vivo.dentro) w.voz_modo(byte);
    },
    tecla(segurando) {
      controles.tecla = Boolean(segurando);
      if (vivo.dentro) w.voz_tecla(controles.tecla ? 1 : 0);
    },
    /** A sensibilidade em milésimos de dBFS; zero é o padrão. */
    regua(milesimos) {
      controles.abertura = milesimos;
      if (vivo.dentro) aplicarRegua();
    },
    ganho(alca, ganho) {
      controles.ganhos.set(alca, ganho);
      mandarAReproducao({ tipo: "ganho", alca, ganho });
    },
    /** Um quadro de som da tela `tela` (o Opus), para a mistura. */
    somDaTela: tocarATela,
    esquecerOSomDaTela,
    /** O volume do som da tela, de 0 a 4; 1 é o de origem. */
    volumeDaTela(volume) {
      somDaTela.volume = Math.min(4, Math.max(0, Number(volume) || 0));
      if (somDaTela.tela !== null) aplicarOGanhoDaTela();
    },
    calarATela(calada) {
      somDaTela.calada = Boolean(calada);
      if (somDaTela.tela !== null) aplicarOGanhoDaTela();
    },
    /** O que o `som_da_tela` do desktop devolve, e quantos quadros tocaram. */
    estadoDoSomDaTela: () => ({ volume: somDaTela.volume, calada: somDaTela.calada, tela: somDaTela.tela, quadros: somDaTela.quadros }),
  };
}
