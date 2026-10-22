// A sessão pelo navegador (Tarefa 2.6 da F4; ADR 0057).
//
// O `connect` da ponte chega aqui com o alvo que a identidade montou (o
// endereço, a impressão do certificado longo, os hashes dos curtos e a chave
// desta pessoa para aquele servidor). Daqui sai uma WebTransport para o mesmo
// socket do `seele/1`, e por ela a mesma sessão do desktop: o servidor serve
// as duas pelo mesmo `session::serve`.
//
// # O que mora aqui, e o que não mora
//
// **Nenhuma linha do protocolo** (`specs/06-clientes-gui.md:19`). Esta camada
// carrega bytes: os do fluxo de controle vão e vêm do `.wasm`
// (`seele_web::ligacao`), que enquadra, confere o carimbo, responde o
// desafio, dobra cada mensagem no `Room` e diz os eventos. Aqui fica só o que
// o navegador tem e o `.wasm` não: a WebTransport, a assinatura pela
// WebCrypto (a chave privada não se exporta, e não passa pelo `.wasm`), o
// relógio, e a voz da página (`voz-web.js`).
//
// Os comandos da sessão (a conversa, a moderação, a sala, as telas) também
// são do `.wasm` (`seele_web::comandos`): a ponte os entrega a `atender`, e a
// resposta, os eventos e o que o fio recusou voltam prontos.
//
// # O que a casca ouve
//
// Os mesmos eventos do desktop, no mesmo canal (`seele://event`), na ordem em
// que o `fold` da FFI os manda: a conformidade dos dois produtores
// (`dois_produtores_de_eventos.rs`) confere. E o retrato é o do desktop: o que
// sai do `Room` vem do `.wasm`, e o que cada ponta mede é medido aqui
// (`MEDIDOS_POR_CADA_PONTA`, em `dois_produtores_de_snapshot.rs`).

/** O prazo do aperto de mão: o `HANDSHAKE_TIMEOUT` do desktop. */
const PRAZO_DO_APERTO_MS = 10_000;

/** O prazo para a recusa chegar antes de largar a conexão (`ESPERA_DA_RECUSA`). */
const ESPERA_DA_RECUSA_MS = 1_000;

/** A volta da medida: o `TICK` do desktop. */
const VOLTA_DA_MEDIDA_MS = 250;

/** O caminho da WebTransport: o `CAMINHO_DO_NAVEGADOR` do `seele-proto`. */
const CAMINHO_DA_SESSAO = "/";

/** As funções da ligação no `.wasm` da página, com a memória por bloco. */
function fronteiraDaLigacao(w) {
  const codificador = new TextEncoder();
  const decodificador = new TextDecoder();
  const memoria = () => new Uint8Array(w.memory.buffer);
  const saida = () => memoria().slice(w.saida_ptr(), w.saida_ptr() + w.saida_len());
  const saidaEmJson = () => JSON.parse(decodificador.decode(saida()));
  function comBytes(bytes, fazer) {
    const p = w.alocar(bytes.length);
    memoria().set(bytes, p);
    try {
      return fazer(p, bytes.length);
    } finally {
      w.liberar(p, bytes.length);
    }
  }
  const comJson = (valor, fazer) => comBytes(codificador.encode(JSON.stringify(valor)), fazer);
  const passos = (codigo) => (codigo === 0 ? saidaEmJson().passos : []);
  return {
    semente: (numero) => w.ligacao_semente(numero),
    /** `null`, ou o que recusou (`{recusa}` ou `{erro}`). */
    abrir: (pedido) => (comJson(pedido, (p, n) => w.ligacao_abrir(p, n)) === 0 ? null : saidaEmJson()),
    chegou: (bytes) => passos(comBytes(bytes, (p, n) => w.ligacao_chegou(p, n))),
    assinado: (bytes) => passos(comBytes(bytes, (p, n) => w.ligacao_assinado(p, n))),
    fluxoAcabou: () => passos(w.ligacao_fluxo_acabou()),
    paraOFio: () => (w.ligacao_para_o_fio() === 0 ? saida() : new Uint8Array(0)),
    retrato: () => (w.ligacao_retrato() === 0 ? saidaEmJson() : null),
    atender(comando, args) {
      const pedido = { comando, args: args ?? {}, agora_s: Math.floor(Date.now() / 1000) };
      const codigo = comJson(pedido, (p, n) => w.ligacao_atender(p, n));
      return { codigo, ...saidaEmJson() };
    },
    ganhosDaVoz: () => (w.voz_ganhos() === 0 ? saidaEmJson() : []),
    fluxoNovo: () => w.fluxo_novo(),
    fluxoChegou: (alca, bytes) => passos(comBytes(bytes, (p, n) => w.fluxo_chegou(alca, p, n))),
    fluxoAcabou: (alca) => passos(w.fluxo_acabou(alca)),
    /** O `Preview` do desktop para um anexo: o pedido e os bytes que chegaram. */
    previa(pedido, bytes) {
      const texto = codificador.encode(JSON.stringify(pedido));
      const p = w.alocar(texto.length);
      memoria().set(texto, p);
      try {
        comBytes(bytes ?? new Uint8Array(0), (pb, nb) => w.previa_julgar(p, texto.length, pb, nb));
      } finally {
        w.liberar(p, texto.length);
      }
      return saidaEmJson();
    },
    /** A queda: a sessão fica guardada, e os eventos dela saem. */
    largar: () => (w.ligacao_largar() === 0 ? saidaEmJson() : []),
    /** A volta: o pedido de antes (sem o convite) e as escolhas; `null`, ou a recusa. */
    retomar(pedido, escolhas) {
      const texto = codificador.encode(JSON.stringify(escolhas));
      const p = w.alocar(texto.length);
      memoria().set(texto, p);
      try {
        const codigo = comJson(pedido, (pp, pn) => w.ligacao_retomar(pp, pn, p, texto.length));
        return codigo === 0 ? null : saidaEmJson();
      } finally {
        w.liberar(p, texto.length);
      }
    },
    bateriaNova: () => w.bateria_nova(),
    bateriaPoll: (ms) => w.bateria_poll(ms),
    bateriaPong: () => w.bateria_pong(),
    bateriaCaiu: (ms) => w.bateria_caiu(ms),
    bateriaVoltou: () => w.bateria_voltou(),
    bateriaFalhou: (ms) => w.bateria_falhou(ms),
    enlace: (ms) => (w.bateria_enlace(ms) === 0 ? saidaEmJson() : "Online"),
    medir: (rttMicros, jitterMs, perda) => w.ligacao_medir(BigInt(Math.max(0, Math.round(rttMicros))), jitterMs, perda) === 1,
    fechar: () => w.ligacao_fechar(),
  };
}

/** Bytes de um hex de 64 dígitos. */
function bytesDoHex(hex) {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i += 1) bytes[i] = Number.parseInt(hex.slice(2 * i, 2 * i + 2), 16);
  return bytes;
}

/** O hex de uns bytes, em minúsculas. */
const hexDe = (bytes) => Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");

/** Uma WebTransport para um endereço, presa aos hashes dos certificados curtos. */
function abrirAWebTransport(endereco, hashes) {
  return new WebTransport(`https://${endereco}${CAMINHO_DA_SESSAO}`, {
    serverCertificateHashes: hashes.map((hex) => ({ algorithm: "sha-256", value: bytesDoHex(hex) })),
    requireUnreliable: true,
  });
}

/**
 * Os hashes que a volta oferece: os do link, e os das janelas anunciadas
 * (`HashesDoNavegador`, os bytes de cada `hash`) que valem em `segundos`, com
 * a folga do relógio da identidade (`FOLGA_DO_RELOGIO_S`, de
 * `identidade-web.js`). Sem repetir.
 */
function hashesDaVolta(doLink, janelas, segundos) {
  const valem = (janelas ?? [])
    .filter((janela) => Array.isArray(janela.hash) && janela.hash.length === 32)
    .filter((janela) => janela.nao_antes - FOLGA_DO_RELOGIO_S <= segundos && segundos <= janela.nao_depois + FOLGA_DO_RELOGIO_S)
    .map((janela) => hexDe(Uint8Array.from(janela.hash)));
  return [...new Set([...valem, ...doLink.map((hex) => hex.toLowerCase())])];
}

/** Espera uma promessa até um prazo; vencido, rejeita com `motivo`. */
function comPrazo(promessa, ms, motivo) {
  let relogio;
  const prazo = new Promise((_, rejeitar) => {
    relogio = setTimeout(() => rejeitar(motivo), ms);
  });
  return Promise.race([promessa, prazo]).finally(() => clearTimeout(relogio));
}

/**
 * Abre a sessão do navegador e resolve quando o servidor a aceitou.
 *
 * - `wasm`: as exportações da instância da página;
 * - `alvo`: o da identidade (`endereco`, `alternativos`, `impressao`,
 *   `hashes`, `chave`);
 * - `apelido`, `convite` (o segredo do convite, ou `null`) e `aceito` (o
 *   conjunto de MODs já aceito neste servidor, ou `null`);
 * - `emitir(evento)`: cada `seele_ffi::Event` para a casca;
 * - `aoAcabar(motivo)`: a sessão acabou depois de aberta;
 * - `aoReceberOsHashes(janelas)`: o `HashesDoNavegador` do servidor;
 * - `transporte(endereco, hashes)`: a WebTransport, trocada na bancada;
 * - `cancelamento`: `{ cancelada, aoCancelar }`, o CANCELAR de quem espera
 *   (`cancelar_ligacao`): a entrada desiste com `"Cancelada"`, como a do
 *   desktop.
 *
 * Rejeita com o `ConnectionError` do desktop.
 */
async function abrirASessaoDoNavegador({
  wasm,
  alvo,
  cancelamento = { cancelada: false, aoCancelar: null },
  apelido,
  convite = null,
  aceito = null,
  emitir = () => {},
  aoAcabar = () => {},
  aoReceberOsHashes = () => {},
  transporte = abrirAWebTransport,
}) {
  const ligacao = fronteiraDaLigacao(wasm);
  ligacao.semente(crypto.getRandomValues(new Uint32Array(1))[0]);
  const chavePublica = new Uint8Array(await crypto.subtle.exportKey("raw", alvo.chave.publicKey));
  const pedido = { apelido, chave_publica: hexDe(chavePublica), impressao: alvo.impressao, aceito };
  const recusa = ligacao.abrir({ ...pedido, convite });
  if (recusa) throw recusa.recusa ?? "Unreachable";
  ligacao.bateriaNova();

  const candidatos = [alvo.endereco, ...(alvo.alternativos ?? [])];
  const etapa = (stage) => emitir({ ConnectStageChanged: { stage } });
  etapa({ Parada: { candidatos: candidatos.length, com_bilhete_e_impressao: false } });

  /** A desistência pelo CANCELAR: a etapa da casca, e a recusa do desktop. */
  const cancelada = () => {
    etapa("Cancelada");
    return "Cancelada";
  };

  let wt = null;
  let atendeu = alvo.endereco;
  for (const [candidato, onde] of candidatos.entries()) {
    if (cancelamento.cancelada) break;
    etapa({ Tentando: { candidato, de: candidatos.length, onde, avisou: false } });
    try {
      const tentativa = transporte(onde, alvo.hashes);
      cancelamento.aoCancelar = () => tentativa.close();
      await comPrazo(tentativa.ready, PRAZO_DO_APERTO_MS, "HandshakeTimeout");
      wt = tentativa;
      atendeu = onde;
      break;
    } catch (falha) {
      if (!cancelamento.cancelada) console.warn(`sessão: ${onde} não abriu a WebTransport:`, falha);
    }
  }
  if (cancelamento.cancelada) {
    ligacao.fechar();
    throw cancelada();
  }
  if (!wt) {
    ligacao.fechar();
    etapa({ Desistiu: { motivo: "Unreachable" } });
    throw "Unreachable";
  }

  // A volta entra pelo endereço que atendeu e com a mesma chave, sem o
  // convite: o servidor já conhece a chave, e o convite é de uso único. Os
  // hashes são os do link e os das janelas que a sessão anunciou e que valem
  // agora: um servidor que reiniciou com o certificado da janela seguinte
  // atende do mesmo jeito.
  let janelas = [];
  const reabrir = () => transporte(atendeu, hashesDaVolta(alvo.hashes, janelas, Date.now() / 1000));
  const lembrar = (anunciadas) => {
    janelas = anunciadas;
    aoReceberOsHashes(anunciadas);
  };
  const sessao = criarASessaoAberta({ ligacao, wt, alvo, pedido, reabrir, emitir, aoAcabar, aoReceberOsHashes: lembrar });
  const desistiu = new Promise((_, rejeitar) => {
    cancelamento.aoCancelar = () => rejeitar("Cancelada");
  });
  try {
    await comPrazo(Promise.race([sessao.aperto, desistiu]), PRAZO_DO_APERTO_MS, "HandshakeTimeout");
  } catch (motivo) {
    await sessao.encerrar();
    if (motivo === "Cancelada") throw cancelada();
    etapa({ Desistiu: { motivo } });
    throw motivo;
  }
  cancelamento.aoCancelar = null;
  etapa("Dentro");
  sessao.comecarAMedir();
  return sessao;
}

/**
 * **A sessão sobre a WebTransport**, que sobrevive à troca dela: o aperto, a
 * leitura, os comandos, e a volta.
 *
 * # A volta (a bateria interna)
 *
 * A queda — o transporte que fecha, ou três `Ping` de rotina sem `Pong` — põe a
 * sessão na bateria do desktop (`seele_sessao::battery`, pelo `.wasm`): a
 * sessão do `.wasm` fica guardada com a conversa, o que estava saindo fica sem
 * resposta, e o retrato segue legível, com o enlace dizendo os segundos que
 * restam. As tentativas seguem o recuo da bateria; cada uma abre outro
 * transporte para o mesmo endereço e refaz o aperto com a mesma chave, sem o
 * convite. A `Session` nova é readotada por cima da sala velha, e o que o motor
 * do desktop restabelece sai junto (a sala, o canal com uma página do buraco,
 * o mudo, o isolamento); depois, as pendentes saem de novo, com as mesmas
 * chaves, na rajada e no passo da FFI. Uma recusa do servidor na volta encerra
 * com o motivo dela; a bateria que descarrega encerra com `LinkLost`.
 *
 * Cada transporte tem a sua geração: o que um velho ainda diz (um fechamento
 * atrasado, um fluxo que termina) não mexe na sessão do novo.
 */
function criarASessaoAberta({ ligacao, wt: primeiro, alvo, pedido, reabrir, emitir, aoAcabar, aoReceberOsHashes }) {
  let wt = null;
  let geracao = 0;
  let escritor = null;
  let dentro = false;
  let acabou = false;
  let voltando = false;
  let tentando = false;
  let medindo = null;
  let ultimoEnlace = 0;
  let resolverOAperto;
  let rejeitarOAperto;
  const aperto = new Promise((resolver, rejeitar) => {
    resolverOAperto = resolver;
    rejeitarOAperto = rejeitar;
  });
  /** A volta em curso: quem espera o `Voltou`, ou a recusa. */
  let volta = null;
  /** Quem espera o peso de cada canal (`peso_da_linha`): `{ resolver, rejeitar }`. */
  const pesos = new Map();
  /** Quem espera cada anexo pedido, pela id: `{ abriu, pedaco, chegou }`. */
  const anexos = new Map();
  /** Quantas mensagens de um vocabulário mais novo foram puladas, e as voltas. */
  const contas = { desconhecidas: 0, naoSairam: 0, voltas: 0, tentativasDeVolta: 0 };
  /** Uma escrita por vez no fluxo de controle, na ordem. */
  let escrita = Promise.resolve();

  function escreverOQueEspera() {
    // Sem fluxo de controle ainda (a volta abrindo), o que espera fica no
    // `.wasm`: o `Hello` da volta não pode ser largado.
    if (!escritor) return escrita;
    const bytes = ligacao.paraOFio();
    if (bytes.length === 0) return escrita;
    const deste = escritor;
    escrita = escrita.then(() => deste.write(bytes)).catch((falha) => {
      if (!acabou && !voltando) console.warn("sessão: a escrita no fluxo de controle falhou:", falha);
    });
    return escrita;
  }

  async function seguirOsPassos(passos) {
    for (const passo of passos) {
      if (passo === "entrou") {
        dentro = true;
        resolverOAperto();
      } else if (passo === "voltou") {
        volta?.resolver();
      } else if (passo.assinar) {
        const assinatura = await crypto.subtle.sign("Ed25519", alvo.chave.privateKey, new Uint8Array(passo.assinar));
        await seguirOsPassos(ligacao.assinado(new Uint8Array(assinatura)));
      } else if (passo.chegou !== undefined) {
        chegou(passo.chegou, passo.eventos);
      } else if (passo.desconhecida !== undefined) {
        contas.desconhecidas += 1;
        if (contas.desconhecidas === 1) {
          console.info(`sessão: o servidor falou uma mensagem mais nova (ordinal ${passo.desconhecida}); pulada`);
        }
      } else if (passo.recusada !== undefined) {
        await escreverOQueEspera();
        if (volta) volta.rejeitar(passo.recusada);
        else rejeitarOAperto(passo.recusada);
      } else if (passo.perdida !== undefined) {
        console.warn("sessão: o fluxo de controle se perdeu:", passo.perdida);
        if (dentro) comecarAVoltar();
      } else if (passo.peso_recusado) {
        // O primeiro pedido daquele canal: a recusa do desktop, com o motivo.
        const { channel, causa } = passo.peso_recusado;
        const fila = pesos.get(channel) ?? [];
        const primeiroDaFila = fila.shift();
        if (fila.length === 0) pesos.delete(channel);
        primeiroDaFila?.rejeitar({ PedidoRecusado: { causa } });
      }
    }
    await escreverOQueEspera();
  }

  function chegou(mensagem, eventos) {
    if (mensagem && mensagem.HashesDoNavegador) aoReceberOsHashes(mensagem.HashesDoNavegador.janelas ?? []);
    // O `Pong` de um `Ping` de rotina (os marcados têm os bits de cima) diz à
    // bateria que o enlace vive.
    if (mensagem && mensagem.Pong && mensagem.Pong.timestamp < 2 ** 62) ligacao.bateriaPong();
    if (mensagem && mensagem.ChannelWeighed) {
      const peso = mensagem.ChannelWeighed;
      for (const { resolver } of pesos.get(peso.channel) ?? []) resolver(peso);
      pesos.delete(peso.channel);
    }
    // O anexo que o servidor não entrega: quem o esperava sabe que não veio.
    if (mensagem && mensagem.AttachmentUnavailable) {
      const id = mensagem.AttachmentUnavailable.attachment;
      anexos.get(id)?.chegou(false);
      anexos.delete(id);
    }
    for (const evento of eventos ?? []) {
      emitir(evento);
      if (evento && evento.Ended) acabar(null);
    }
    sessao.aoDobrar?.(mensagem);
  }

  /** Liga a sessão a um transporte: o fluxo de controle, os fluxos do servidor e a voz. */
  function ligarOTransporte(novo) {
    const minha = ++geracao;
    wt = novo;
    escritor = null;
    escrita = Promise.resolve();
    escritorDaVoz = null;
    const vale = () => minha === geracao && !acabou;
    (async () => {
      const fluxo = await novo.createBidirectionalStream();
      if (!vale()) return;
      escritor = fluxo.writable.getWriter();
      escreverOQueEspera();
      const leitor = fluxo.readable.getReader();
      try {
        for (;;) {
          const { value, done } = await leitor.read();
          if (done || !vale()) break;
          await seguirOsPassos(ligacao.chegou(value));
        }
      } catch (falha) {
        if (vale() && !voltando) console.warn("sessão: a leitura do fluxo de controle parou:", falha);
      }
      if (vale()) await seguirOsPassos(ligacao.fluxoAcabou());
      if (!dentro) rejeitarOAperto("Unreachable");
    })().catch((falha) => {
      if (!vale()) return;
      console.warn("sessão: o fluxo de controle não abriu:", falha);
      if (!dentro) rejeitarOAperto("Unreachable");
      else if (volta) volta.rejeitar("Unreachable");
    });
    novo.closed.then(
      () => caiu(minha),
      () => caiu(minha),
    );
    (async () => {
      try {
        const leitor = novo.incomingUnidirectionalStreams.getReader();
        for (;;) {
          const { value: fluxo, done } = await leitor.read();
          if (done || !vale()) break;
          lerUmFluxo(fluxo);
        }
      } catch {
        // O transporte acabou; quem diz o que fazer é `caiu`.
      }
    })();
    (async () => {
      try {
        // Antes de qualquer espera: o `wt` aqui ainda é o `novo` desta ligação.
        const leitor = wt.datagrams.readable.getReader();
        for (;;) {
          const { value, done } = await leitor.read();
          if (done || !vale()) break;
          ouvinteDaVoz?.(value);
        }
      } catch {
        // O transporte acabou; quem diz o que fazer é `caiu`.
      }
    })();
  }

  /** O transporte `minha` fechou. */
  function caiu(minha) {
    if (acabou || minha !== geracao) return;
    if (!dentro) {
      rejeitarOAperto("Unreachable");
      return;
    }
    if (volta) volta.rejeitar("Unreachable");
    else comecarAVoltar();
  }

  /** **A queda**: a bateria começa, e a sessão fica guardada para a volta. */
  function comecarAVoltar() {
    if (acabou || voltando) return;
    voltando = true;
    for (const evento of ligacao.largar()) emitir(evento);
    ligacao.bateriaCaiu(performance.now());
    geracao += 1;
    try {
      wt?.close();
    } catch {
      // Já fechado.
    }
    // Os pedidos que esperavam a conexão que caiu não vão ter resposta dela.
    for (const [, esperando] of anexos) esperando.acabou?.();
    anexos.clear();
    for (const [, esperando] of pesos) for (const { resolver } of esperando) resolver(null);
    pesos.clear();
    sessao.aoCair?.();
    emitir("TelemetryChanged");
  }

  /** **Uma tentativa de volta**: outro transporte, o aperto com a sessão guardada. */
  async function tentarVoltar() {
    tentando = true;
    contas.tentativasDeVolta += 1;
    let novo = null;
    try {
      novo = reabrir();
      await comPrazo(novo.ready, PRAZO_DO_APERTO_MS, "HandshakeTimeout");
      if (acabou) return;
      const recusa = ligacao.retomar(pedido, sessao.escolhas());
      if (recusa) throw recusa.recusa ?? "Unreachable";
      const esperando = new Promise((resolver, rejeitar) => {
        volta = { resolver, rejeitar };
      });
      ligarOTransporte(novo);
      await comPrazo(esperando, PRAZO_DO_APERTO_MS, "HandshakeTimeout");
      volta = null;
      voltando = false;
      contas.voltas += 1;
      ligacao.bateriaVoltou();
      emitir("TelemetryChanged");
      sessao.aoVoltar?.();
      repetirAsPendentes();
    } catch (motivo) {
      volta = null;
      geracao += 1;
      try {
        novo?.close();
      } catch {
        // Já fechado.
      }
      if (acabou) return;
      // O servidor que recusa a volta encerra a sessão com o motivo dele; o
      // resto é mais uma tentativa que a bateria conta.
      const fim = motivoDoFimNaVolta(motivo);
      if (fim) {
        acabar(fim);
        return;
      }
      ligacao.largar();
      ligacao.bateriaFalhou(performance.now());
    } finally {
      tentando = false;
    }
  }

  /**
   * As pendentes que a volta repete: a rajada, e depois uma a cada passo, que
   * o `.wasm` diz (o `INTERVALO_DA_REPETICAO` do `seele_sessao::volta`).
   */
  function repetirAsPendentes() {
    if (acabou || voltando) return;
    const feito = sessao.atender("repetir_pendentes", {});
    if (feito?.resposta?.faltam > 0) setTimeout(repetirAsPendentes, feito.resposta.passo_ms);
  }

  async function lerUmFluxo(fluxo) {
    const alca = ligacao.fluxoNovo();
    const leitor = fluxo.getReader();
    let anexo = null;
    const seguir = (passos, pedaco) => {
      for (const passo of passos) {
        if (passo.evento !== undefined) emitir(passo.evento);
        else if (passo.som) sessao.aoSomDaTela?.(passo.som.tela, passo.som.opus);
        else if (passo.anexo_abriu) {
          anexo = { id: passo.anexo_abriu.anexo, ...(anexos.get(passo.anexo_abriu.anexo) ?? {}) };
          anexo.abriu?.(passo.anexo_abriu.tamanho, () => leitor.cancel().catch(() => {}));
        } else if (passo.bytes) anexo?.pedaco?.(pedaco.slice(passo.bytes[0], passo.bytes[1]));
        else if (passo.anexo_chegou) {
          anexos.get(passo.anexo_chegou.anexo)?.chegou(passo.anexo_chegou.inteiro);
          anexos.delete(passo.anexo_chegou.anexo);
          anexo = null;
        } else if (passo.ignorado) console.warn("sessão: um fluxo do servidor foi ignorado:", passo.ignorado);
      }
    };
    try {
      for (;;) {
        const { value, done } = await leitor.read();
        if (done) break;
        seguir(ligacao.fluxoChegou(alca, value), value);
      }
    } catch {
      // O fluxo caiu, ou quem o lia o cancelou: o que faltava é dito abaixo.
    }
    if (!acabou) seguir(ligacao.fluxoAcabou(alca), null);
  }

  /** O fim, uma vez só: a casca ouve o `Ended` (do servidor, ou a queda). */
  function acabar(motivo) {
    if (acabou) return;
    acabou = true;
    if (medindo) clearInterval(medindo);
    for (const [, esperando] of pesos) for (const { resolver } of esperando) resolver(null);
    pesos.clear();
    for (const [, esperando] of anexos) esperando.acabou?.();
    anexos.clear();
    if (dentro && motivo) emitir({ Ended: { reason: motivo } });
    try {
      wt?.close();
    } catch {
      // Já fechada.
    }
    ligacao.fechar();
    if (dentro) aoAcabar(motivo);
  }

  /** Os datagramas da voz, no contrato do `voz-web.js`, pelo transporte de agora. */
  let ouvinteDaVoz = null;
  let escritorDaVoz = null;
  const transporteDaVoz = Object.freeze({
    enviar(bytes) {
      if (acabou || voltando || !wt) return false;
      escritorDaVoz ??= wt.datagrams.writable.getWriter();
      escritorDaVoz.write(bytes).catch(() => {});
      return true;
    },
    ouvir(aoChegar) {
      ouvinteDaVoz = aoChegar;
      return () => {
        if (ouvinteDaVoz === aoChegar) ouvinteDaVoz = null;
      };
    },
  });

  /** O tempo de ida e volta que a WebTransport estimou, em microssegundos. */
  async function idaEVolta() {
    try {
      const estatisticas = await wt.getStats();
      const ms = estatisticas.smoothedRtt ?? estatisticas.minRtt;
      return Number.isFinite(ms) ? ms * 1000 : 0;
    } catch {
      return 0;
    }
  }

  /** A volta da medida: o sinal, a bateria (o ping, a volta, o fim) e a contagem. */
  async function umaVolta() {
    if (acabou) return;
    const agora = performance.now();
    const acao = ligacao.bateriaPoll(agora);
    if (acao === 1 && !voltando) sessao.atender("ping_de_rotina", {});
    else if (acao === 2 && voltando && !tentando) tentarVoltar();
    else if (acao === 3) {
      acabar("LinkLost");
      return;
    }
    const enlace = ligacao.enlace(agora);
    // Três `Ping` sem `Pong`: a bateria percebeu antes do transporte.
    if (!voltando && enlace !== "Online") comecarAVoltar();
    if (voltando) {
      // A contagem regressiva é a informação: uma vez por segundo.
      if (agora - ultimoEnlace >= 1000) {
        ultimoEnlace = agora;
        emitir("TelemetryChanged");
      }
      return;
    }
    const { jitterMs, perda } = sessao.medidas();
    if (ligacao.medir(await idaEVolta(), jitterMs, perda)) emitir("TelemetryChanged");
    sessao.aCadaVolta?.();
  }

  const sessao = {
    aperto,
    contas,
    transporteDaVoz,
    get acabou() {
      return acabou;
    },
    get voltando() {
      return voltando;
    },
    /** Quem a ponte põe para ouvir cada mensagem dobrada (a voz, a tela). */
    aoDobrar: null,
    /** O que a ponte faz na queda e na volta (a voz). */
    aoCair: null,
    aoVoltar: null,
    /** O mudo e o isolamento que a volta repete; a ponte o troca. */
    escolhas: () => ({ mudo: false, isolado: false }),
    /** O que esta ponta mede, para o retrato; a ponte o troca. */
    medidas: () => ({ jitterMs: 0, perda: 0 }),
    /** O que a ponte faz a cada volta da medida (os volumes, a voz). */
    aCadaVolta: null,
    comecarAMedir() {
      medindo = setInterval(umaVolta, VOLTA_DA_MEDIDA_MS);
    },
    /** O enlace como o `Snapshot` o diz: `"Online"`, ou a bateria. */
    enlace: () => ligacao.enlace(performance.now()),
    /**
     * Fecha o transporte de agora por baixo da sessão, como a rede que cai: a
     * bancada o usa, e a queda segue pelo caminho de verdade (o `closed`).
     */
    simularAQueda: () => wt?.close(),
    /** O ganho de cada voz com volume escolhido, pela alça da mistura. */
    ganhosDaVoz: () => (acabou || voltando ? [] : ligacao.ganhosDaVoz()),
    /** O retrato do `.wasm`: a sala, os contados e a telemetria. */
    retrato: () => ligacao.retrato(),
    /**
     * Um comando da sessão: `{ resposta }`, `{ erro }`, ou `null` quando ele
     * não é da sessão. Os eventos saem para a casca antes de a resposta voltar.
     * Na bateria, o `.wasm` atende pela sessão guardada e não escreve nada.
     */
    atender(comando, args) {
      if (acabou) return { erro: "NotConnected" };
      const lido = ligacao.atender(comando, args);
      if (lido.codigo === 2) return null;
      if (lido.codigo === 1) return { erro: lido.erro };
      if (lido.nao_saiu) {
        contas.naoSairam += 1;
        console.warn(`sessão: \`${comando}\`: ${lido.nao_saiu}`);
      }
      escreverOQueEspera();
      for (const evento of lido.eventos ?? []) emitir(evento);
      return { resposta: lido.resposta };
    },
    /**
     * Espera o `ChannelWeighed` de um canal; `null` se a sessão acabar antes,
     * e a rejeição do desktop se o servidor recusar o pedido.
     */
    esperarOPeso(canal) {
      return new Promise((resolver, rejeitar) => {
        if (acabou) return resolver(null);
        const lista = pesos.get(canal) ?? [];
        lista.push({ resolver, rejeitar });
        pesos.set(canal, lista);
      });
    },
    /**
     * Quem espera um anexo pedido (`pedir_anexo`): `abriu(tamanho, cancelar)`,
     * `pedaco(bytes)`, `chegou(inteiro)` e `acabou()`, a sessão que caiu.
     * Guardado **antes** de pedir.
     */
    receberOAnexo(id, quem) {
      anexos.set(id, quem);
    },
    /** Um fluxo unidirecional para o servidor: o envio de um anexo. */
    abrirUmFluxo: () => wt.createUnidirectionalStream(),
    /**
     * Um fluxo bidirecional para o servidor: uma imagem de MOD pelo volume.
     * Sem enlace (a bateria, ou o fim), `sessao-encerrada`, como o motor do
     * desktop sem cliente.
     */
    abrirUmFluxoDeImagem: () =>
      acabou || voltando || !wt ? Promise.reject("sessao-encerrada") : wt.createBidirectionalStream(),
    /** O `Preview` do desktop, pelo juiz do `.wasm`. */
    previa: (pedidoDaPrevia, bytes) => ligacao.previa(pedidoDaPrevia, bytes),
    /**
     * Fecha a sessão, sem `Ended`: quem fecha é esta ponta (o `disconnect`, a
     * recusa, o prazo do aperto, o CANCELAR), e a casca já sabe.
     */
    async encerrar() {
      if (!acabou && escritor && !voltando) {
        await escreverOQueEspera();
        try {
          await comPrazo(escritor.close(), ESPERA_DA_RECUSA_MS, "prazo");
        } catch {
          // A recusa que não chegou no prazo: largamos mesmo assim.
        }
      }
      acabar(null);
    },
  };
  ligarOTransporte(primeiro);
  return sessao;
}

/**
 * O fim de uma sessão que o servidor recusou na volta, como o `fim_da_recusa`
 * da FFI: a recusa dele, os MODs que mudaram, a outra linha. `null` para o que
 * é só mais uma tentativa (o transporte que não abriu, o prazo).
 */
function motivoDoFimNaVolta(motivo) {
  if (motivo && motivo.Refused) return motivo.Refused.reason;
  if (motivo && motivo.ModsNaoAceitos) return "ModsMudaram";
  if (motivo && motivo.OutraLinha) return "Incompatible";
  return null;
}
