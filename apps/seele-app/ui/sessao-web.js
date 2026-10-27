// A sessão pelo navegador (Tarefa 2.6 da F4; ADR 0057; na 0.17, WebRTC).
//
// O `connect` da ponte chega aqui com o alvo que a identidade montou (o
// endereço e os alternativos, a impressão do servidor, o bilhete do encontro e
// a chave desta pessoa para aquele servidor). Daqui sai um enlace WebRTC
// (`rtc-web.js`) para o socket do anfitrião, e por ele a mesma sessão do
// desktop: o servidor serve as duas pelo mesmo `session::serve`.
//
// # O que mora aqui, e o que não mora
//
// **Nenhuma linha do protocolo** (`specs/06-clientes-gui.md:19`). Esta camada
// carrega bytes: os do fluxo de controle vão e vêm do `.wasm`
// (`seele_web::ligacao`), que enquadra, confere o carimbo, responde o
// desafio, dobra cada mensagem no `Room` e diz os eventos. Aqui fica só o que
// o navegador tem e o `.wasm` não: o enlace, a assinatura pela WebCrypto (a
// chave privada não se exporta, e não passa pelo `.wasm`), o relógio, e a voz
// da página (`voz-web.js`).
//
// Os comandos da sessão (a conversa, a moderação, a sala, as telas) também
// são do `.wasm` (`seele_web::comandos`): a ponte os entrega a `atender`, e a
// resposta, os eventos e o que o fio recusou voltam prontos.
//
// # Várias sessões (E3)
//
// **Sem estado de módulo.** Cada sessão tem o próprio enlace, a própria
// instância do `.wasm` de protocolo (`instanciarOWasmDaSessao`), a chave do
// servidor dela, a volta, as filas e os eventos. O registro
// (`criarRegistroDeSessoesWeb`, no fim deste arquivo) as junta, carimba cada
// evento com a sessão e a geração, e guarda **o foco de voz único**: a voz só
// sai pelo transporte da sessão em foco, e só a voz dela chega à mistura.
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

/** As funções da ligação no `.wasm` da sessão, com a memória por bloco. */
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
    /**
     * O fluxo de CONTROLE acabou, com o código do fechamento quando ele veio
     * (`{corte}` da leitura ou `{fechada}` do `closed`): o `.wasm` traduz o
     * `CODIGO_FICOU_PARA_TRAS` na despedida `FellBehind`, uma vez por conexão.
     * (Antes este nome era sobrescrito pelo `fluxoAcabou` dos fluxos do
     * servidor, logo abaixo, e o fim do controle nunca chegava ao `.wasm`.)
     */
    controleAcabou: (codigo) =>
      passos(
        typeof codigo === "number" && typeof w.ligacao_fluxo_acabou_com === "function"
          ? w.ligacao_fluxo_acabou_com(codigo >>> 0)
          : w.ligacao_fluxo_acabou(),
      ),
    paraOFio: () => (w.ligacao_para_o_fio() === 0 ? saida() : new Uint8Array(0)),
    retrato: () => (w.ligacao_retrato() === 0 ? saidaEmJson() : null),
    atender(comando, args) {
      const pedido = { comando, args: args ?? {}, agora_s: Math.floor(Date.now() / 1000) };
      const codigo = comJson(pedido, (p, n) => w.ligacao_atender(p, n));
      return { codigo, ...saidaEmJson() };
    },
    ganhosDaVoz: () => (w.voz_ganhos() === 0 ? saidaEmJson() : []),
    /** Os ganhos desta sessão num bloco opaco, para a recepção da página traduzir. */
    ganhosOpacos: () => (typeof w.voz_ganhos_da_sessao === "function" && w.voz_ganhos_da_sessao() === 0 ? saida() : new Uint8Array(0)),
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
    /** A vez do relógio: a página do buraco, a cerca vencida e as pendentes da rajada. */
    vez: (ms, segundos) => (w.ligacao_vez(ms, segundos), saidaEmJson()),
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

/**
 * As funções do sinal (`POST /sinal/1`) no `.wasm`: a página não monta nem lê
 * linha do protocolo do ponto (`seele_web::sinal_chama`, `sinal_resposta`).
 */
/**
 * **Qual leitura de cada tela é a vigente.** Cada fluxo do servidor tem uma
 * alça (`fluxo_novo`), a identidade da leitura, como o `leitura: u64` do
 * nativo. O `ScreenOpened` de uma alça a torna a vigente daquela tela; o
 * `ScreenFrame` e o `ScreenClosed` de qualquer outra
 * alça da mesma tela são de uma leitura substituída e não chegam à página:
 * nem apagam a tela nova, nem pintam, nem entram no decoder dela. A sessão
 * Rust já ignora o fim da leitura velha (`ouvir_o_fluxo`); isto fecha o resto.
 * A entrada fica depois do `ScreenClosed` de propósito: o quadro tardio de
 * uma leitura velha, com a tela já fechada, também é descartado. Um evento de
 * tela sem abertura vista (nenhuma vigente) passa como sempre passou.
 */
function criarOFiltroDasLeituras(registrar = (texto) => console.debug(texto)) {
  const vigente = new Map();
  return {
    /** O som da tela só segue se a alça é a vigente dela (o `TelaSom` do nativo). */
    valeOSom(alca, tela) {
      const atual = vigente.get(tela);
      if (atual === undefined || atual === alca) return true;
      registrar(`sessão: som da tela ${tela} descartado: alça ${alca} substituída pela ${atual}`);
      return false;
    },
    vale(alca, evento) {
      if (!evento || typeof evento !== "object") return true;
      if (evento.ScreenOpened) {
        vigente.set(evento.ScreenOpened.screen, alca);
        return true;
      }
      for (const nome of ["ScreenFrame", "ScreenClosed"]) {
        const corpo = evento[nome];
        if (!corpo) continue;
        const atual = vigente.get(corpo.screen);
        if (atual === undefined || atual === alca) return true;
        registrar(`sessão: ${nome} da tela ${corpo.screen} descartado: alça ${alca} substituída pela ${atual}`);
        return false;
      }
      return true;
    },
  };
}

function fronteiraDoSinal(w) {
  const codificador = new TextEncoder();
  const decodificador = new TextDecoder();
  function chamar(funcao, texto) {
    const bytes = codificador.encode(texto);
    const p = w.alocar(bytes.length);
    new Uint8Array(w.memory.buffer).set(bytes, p);
    try {
      funcao(p, bytes.length);
    } finally {
      w.liberar(p, bytes.length);
    }
    return JSON.parse(decodificador.decode(new Uint8Array(w.memory.buffer, w.saida_ptr(), w.saida_len()).slice()));
  }
  return {
    montar: (pedido) => chamar(w.sinal_chama, JSON.stringify(pedido)),
    ler: (texto) => chamar(w.sinal_resposta, texto),
  };
}

/**
 * **Uma instância própria do `.wasm` de protocolo para uma sessão** (E3): o
 * estado da ligação mora dentro dela, e duas sessões numa instância só
 * dividiriam a ligação. Compilada a partir dos mesmos bytes da página.
 */
async function instanciarOWasmDaSessao() {
  const { bytes } = await carregarOWasmDaPagina();
  return (await WebAssembly.instantiate(bytes, {})).instance.exports;
}

/** O hex de uns bytes, em minúsculas. */
const hexDe = (bytes) => Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");

/**
 * O enlace de uma tentativa, com o certificado DTLS já preparado: o padrão do
 * parâmetro `transporte` (a bancada o troca).
 */
function abrirOEnlace({ alvos, alvo, preparo, sinal, aoMudar }) {
  return criarEnlaceRtc({
    alvos,
    impressao: alvo.impressao,
    certificado: preparo,
    bilhete: alvo.bilhete ?? null,
    sinal,
    urlDoSinal: alvo.urlDoSinal ?? urlDoSinalDoPonto,
    aoMudar,
  });
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
 * O motivo que a casca lê de uma falha do enlace: o `ConnectionError` do
 * desktop quando ele existe (`HandshakeTimeout`, `Unreachable`), ou
 * `{ FalhaDoEnlace: { motivo, detalhe } }`, com o motivo de `FALHA_DO_ENLACE`
 * (`rtc-web.js`) e a frase dele no bloco do transporte da `frases.js`.
 */
function motivoDaFalhaDoEnlace(falha) {
  if (falha && falha.FalhaDoEnlace) {
    if (falha.FalhaDoEnlace.motivo === FALHA_DO_ENLACE.prazo) return "HandshakeTimeout";
    return falha;
  }
  if (falha === FALHA_DO_ENLACE.semWebRtc) return { FalhaDoEnlace: { motivo: falha, detalhe: null } };
  return typeof falha === "string" ? falha : "Unreachable";
}

/**
 * Abre a sessão do navegador e resolve quando o servidor a aceitou.
 *
 * - `wasm`: as exportações da instância **desta sessão**
 *   (`instanciarOWasmDaSessao`); duas sessões nunca dividem uma;
 * - `alvo`: o da identidade (`endereco`, `alternativos`, `impressao`,
 *   `bilhete` e `chave`);
 * - `apelido`, `convite` (o segredo do convite, ou `null`) e `aceito` (o
 *   conjunto de MODs já aceito neste servidor, ou `null`);
 * - `emitir(evento)`: cada `seele_ffi::Event` para a casca;
 * - `aoAcabar(motivo)`: a sessão acabou depois de aberta;
 * - `podeUsarAVoz()`: se esta sessão tem o foco de voz (o registro o dá); sem
 *   registro, sempre;
 * - `transporte(opcoes)`: o enlace, trocado na bancada;
 * - `cancelamento`: `{ cancelada, aoCancelar }`, o CANCELAR de quem espera
 *   (`cancelar_ligacao`): a entrada desiste com `"Cancelada"`, como a do
 *   desktop.
 *
 * Rejeita com o `ConnectionError` do desktop, ou com o `FalhaDoEnlace` da
 * página (`motivoDaFalhaDoEnlace`).
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
  podeUsarAVoz = () => true,
  transporte = abrirOEnlace,
}) {
  const ligacao = fronteiraDaLigacao(wasm);
  const sinal = typeof wasm.sinal_chama === "function" ? fronteiraDoSinal(wasm) : null;
  ligacao.semente(crypto.getRandomValues(new Uint32Array(1))[0]);
  const etapa = (stage) => emitir({ ConnectStageChanged: { stage } });

  // O certificado DTLS desta conexão nasce antes do `Hello`: a impressão dele
  // vai na prova D5, e o `Hello` espera no `.wasm` até o enlace abrir.
  let preparo;
  try {
    preparo = await prepararCertificadoDtls();
  } catch (falha) {
    throw motivoDaFalhaDoEnlace(falha);
  }
  const chavePublica = new Uint8Array(await crypto.subtle.exportKey("raw", alvo.chave.publicKey));
  const pedido = {
    apelido,
    chave_publica: hexDe(chavePublica),
    impressao: alvo.impressao,
    impressao_dtls: preparo.impressao,
    aceito,
  };
  const recusa = ligacao.abrir({ ...pedido, convite });
  if (recusa) throw recusa.recusa ?? "Unreachable";
  ligacao.bateriaNova();

  const alvos = [alvo.endereco, ...(alvo.alternativos ?? [])];
  etapa({ Parada: { candidatos: alvos.length, com_bilhete_e_impressao: Boolean(alvo.bilhete) } });

  /** A desistência pelo CANCELAR: a etapa da casca, e a recusa do desktop. */
  const cancelada = () => {
    etapa("Cancelada");
    return "Cancelada";
  };

  // **Um enlace só, com todos os candidatos no mesmo agente ICE**: a corrida
  // de um por vez da WebTransport acabou. O ICE tenta os pares juntos, e o
  // ponto só é chamado se o caminho direto não abrir logo (D9).
  const aoMudar = (mudanca) => {
    if (mudanca.Discando) etapa({ Tentando: { candidato: 0, de: alvos.length, onde: alvo.endereco, avisou: false } });
    else if (mudanca.ChamandoPeloPonto) etapa({ Tentando: { candidato: 0, de: alvos.length, onde: alvo.endereco, avisou: true } });
  };
  if (cancelamento.cancelada) {
    ligacao.fechar();
    throw cancelada();
  }
  const wt = transporte({ alvos, alvo, preparo, sinal, aoMudar });
  cancelamento.aoCancelar = () => wt.close();
  try {
    await wt.ready;
  } catch (falha) {
    ligacao.fechar();
    if (cancelamento.cancelada) throw cancelada();
    const motivo = motivoDaFalhaDoEnlace(falha);
    console.warn(`sessão: o enlace com ${alvo.endereco} não abriu:`, falha);
    etapa({ Desistiu: { motivo: typeof motivo === "string" ? motivo : "Unreachable" } });
    throw motivo;
  }
  if (cancelamento.cancelada) {
    wt.close();
    ligacao.fechar();
    throw cancelada();
  }

  // A volta entra pelo endereço que atendeu (e os outros, atrás dele), com a
  // mesma chave e sem o convite: o servidor já conhece a chave, e o convite é
  // de uso único. Cada volta tem um certificado DTLS novo, e a prova D5 dela
  // leva a impressão nova.
  const atendeu = wt.atendeuPor ?? alvo.endereco;
  const alvosDaVolta = [...new Set([atendeu, ...alvos])];
  const reabrir = async () => {
    const novoPreparo = await prepararCertificadoDtls();
    pedido.impressao_dtls = novoPreparo.impressao;
    return transporte({ alvos: alvosDaVolta, alvo, preparo: novoPreparo, sinal, aoMudar: () => {} });
  };
  const sessao = criarASessaoAberta({ ligacao, wt, alvo, pedido, reabrir, emitir, aoAcabar, podeUsarAVoz });
  sessao.wasm = wasm;
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
 * **A sessão sobre o enlace**, que sobrevive à troca dele: o aperto, a
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
function criarASessaoAberta({ ligacao, wt: primeiro, alvo, pedido, reabrir, emitir, aoAcabar, podeUsarAVoz = () => true }) {
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
  /** As tentativas de volta falham enquanto isto vale (a bancada o liga). */
  let voltaSegura = false;
  /** Quem espera o peso de cada canal (`peso_da_linha`): `{ resolver, rejeitar }`. */
  const pesos = new Map();
  /** Quem espera cada anexo pedido, pela id: `{ abriu, pedaco, chegou }`. */
  const anexos = new Map();
  /** Quantas mensagens de um vocabulário mais novo foram puladas, e as voltas. */
  const contas = {
    desconhecidas: 0,
    naoSairam: 0,
    voltas: 0,
    tentativasDeVolta: 0,
    // A voz desta sessão, contada pelo transporte (a prova do foco único).
    vozEnviada: 0,
    vozRecusadaSemFoco: 0,
    vozRecebida: 0,
    vozCaladaSemFoco: 0,
    // As despedidas `FellBehind` dobradas pelo CÓDIGO do fechamento (sem o
    // quadro): uma por conexão que ficou para trás.
    ficouParaTras: 0,
  };
  /** Qual alça de fluxo é a leitura vigente de cada tela (ver `criarOFiltroDasLeituras`). */
  const leituras = criarOFiltroDasLeituras();
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
      } else if (passo.buraco) {
        // O que o `seguir` do desktop diz no `seele.log`: o buraco que fechou,
        // ou o pedaço que ficou de fora.
        const { alerta, ...buraco } = passo.buraco;
        (alerta ? console.warn : console.info)("sessão: o buraco do histórico:", buraco);
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
    // **Uma causa por conexão** (a auditoria Astra, item 4): o código do
    // fechamento chega por duas rotas — a leitura do controle que falha com
    // `{corte}`, e o `closed` que resolve com `{fechada}` — e a primeira que o
    // trouxer o guarda para ESTA geração, antes de a queda mudar de geração.
    let codigoDaQueda = null;
    const guardarOCodigo = (codigo) => {
      if (codigoDaQueda === null && typeof codigo === "number") codigoDaQueda = codigo;
    };
    (async () => {
      const fluxo = novo.controle();
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
        guardarOCodigo(falha?.corte);
        if (vale() && !voltando) console.warn("sessão: a leitura do fluxo de controle parou:", falha);
      }
      if (vale()) await seguirOsPassos(controleAcabou(codigoDaQueda));
      if (!dentro) rejeitarOAperto("Unreachable");
    })().catch((falha) => {
      if (!vale()) return;
      console.warn("sessão: o fluxo de controle não abriu:", falha);
      if (!dentro) rejeitarOAperto("Unreachable");
      else if (volta) volta.rejeitar("Unreachable");
    });
    novo.closed.then(
      (motivo) => {
        guardarOCodigo(motivo?.fechada);
        // A mesma causa pela outra rota: se ela chega primeiro, o `.wasm` a
        // ouve antes de a queda começar a volta.
        if (dentro && minha === geracao && !acabou && !voltando) void seguirOsPassos(controleAcabou(codigoDaQueda));
        caiu(minha);
      },
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
        const leitor = wt.canalDaVoz.readable.getReader();
        for (;;) {
          const { value, done } = await leitor.read();
          if (done || !vale()) break;
          // Fora do foco de voz, a voz desta sessão não toca (fronteiras.md,
          // §2): ela é contada, e não chega à mistura.
          if (!podeUsarAVoz()) {
            contas.vozCaladaSemFoco += 1;
            continue;
          }
          contas.vozRecebida += 1;
          ouvinteDaVoz?.(value);
        }
      } catch {
        // O transporte acabou; quem diz o que fazer é `caiu`.
      }
    })();
  }

  /** O fim do controle no `.wasm`, contando a despedida `FellBehind` que o código trouxe. */
  function controleAcabou(codigo) {
    const lidos = ligacao.controleAcabou(codigo);
    for (const passo of lidos) {
      if (passo?.chegou?.Disconnecting?.reason === "FellBehind") contas.ficouParaTras += 1;
    }
    return lidos;
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
      if (voltaSegura) throw "Unreachable";
      novo = await reabrir();
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

  async function lerUmFluxo(fluxo) {
    const alca = ligacao.fluxoNovo();
    const leitor = fluxo.getReader();
    let anexo = null;
    const seguir = (passos, pedaco) => {
      for (const passo of passos) {
        if (passo.evento !== undefined) {
          // Só chega à página o que vem da leitura vigente da tela.
          if (leituras.vale(alca, passo.evento)) emitir(passo.evento);
        }
        else if (passo.som) {
          if (leituras.valeOSom(alca, passo.som.tela)) sessao.aoSomDaTela?.(passo.som.tela, passo.som.opus);
        }
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
      // **O microfone só vai à sessão em foco**, nunca a várias: o portão é
      // aqui, no transporte, e vale ainda que quem captura erre de sessão.
      if (!podeUsarAVoz()) {
        contas.vozRecusadaSemFoco += 1;
        return false;
      }
      escritorDaVoz ??= wt.canalDaVoz.writable.getWriter();
      escritorDaVoz.write(bytes).catch(() => {});
      contas.vozEnviada += 1;
      return true;
    },
    ouvir(aoChegar) {
      ouvinteDaVoz = aoChegar;
      return () => {
        if (ouvinteDaVoz === aoChegar) ouvinteDaVoz = null;
      };
    },
  });

  /** O tempo de ida e volta que o ICE mediu, em microssegundos. */
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
    // A vez do relógio: a busca do buraco e a repetição das pendentes, no
    // compasso do desktop.
    if (dentro) {
      const vez = ligacao.vez(agora, Date.now() / 1000);
      for (const evento of vez.eventos) emitir(evento);
      await seguirOsPassos(vez.passos);
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
    /** A geração do enlace: 1 na entrada, e mais 1 a cada volta que deu certo. */
    get geracaoDoEnlace() {
      return contas.voltas + 1;
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
    /**
     * Faz as tentativas de volta falharem enquanto `segura` (a bancada o usa
     * para a página ficar fora do ar enquanto outra fala): cada uma conta na
     * bateria, como o transporte que não abre.
     */
    segurarAVolta: (segura) => {
      voltaSegura = Boolean(segura);
    },
    /** O ganho de cada voz com volume escolhido, pela alça da mistura. */
    ganhosDaVoz: () => (acabou || voltando ? [] : ligacao.ganhosDaVoz()),
    /**
     * **O ganho de cada voz pela alça da mistura, com uma instância por
     * sessão** (E3): os volumes estão na instância desta sessão, e as alças na
     * recepção da instância da página (`wasmDaPagina`, a da `voz-web.js`). O
     * bloco entre as duas é opaco para o JavaScript.
     */
    ganhosNaMistura(wasmDaPagina) {
      if (acabou || voltando || typeof wasmDaPagina?.voz_ganhos_pela_recepcao !== "function") return [];
      const bloco = ligacao.ganhosOpacos();
      if (bloco.length === 0) return [];
      const p = wasmDaPagina.alocar(bloco.length);
      new Uint8Array(wasmDaPagina.memory.buffer).set(bloco, p);
      try {
        wasmDaPagina.voz_ganhos_pela_recepcao(p, bloco.length);
      } finally {
        wasmDaPagina.liberar(p, bloco.length);
      }
      const saida = new Uint8Array(wasmDaPagina.memory.buffer, wasmDaPagina.saida_ptr(), wasmDaPagina.saida_len()).slice();
      return JSON.parse(new TextDecoder().decode(saida));
    },
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

/**
 * **O registro das sessões da página** (E3; `fronteiras.md`, §1 a §4): várias
 * sessões cliente ao mesmo tempo, cada uma com a sua, e **um foco de voz
 * só**.
 *
 * - Cada sessão tem um `id` que nasce na tentativa, sobe sempre, nunca volta e
 *   nunca é 0. Cada evento sai carimbado: `{ sessao, geracao, evento }`, com a
 *   geração do enlace. Um evento de uma sessão que já saiu do registro é
 *   largado aqui, e não chega à casca.
 * - **À frente** está a que a janela mostra; as outras ficam **ao fundo**,
 *   conectadas, recebendo texto e presença. `ver(id)` troca a vista sem
 *   derrubar ninguém. Uma entrada nova vem à frente, e a anterior vai ao fundo
 *   inteira.
 * - **O foco de voz** é de no máximo uma sessão, e passa por **um coordenador
 *   só**: toda troca, toda saída e toda queda entram na mesma cadeia, na ordem.
 *   - Numa troca, primeiro ninguém tem o foco (o portão fecha na hora), a voz
 *     da antiga para (`voz.soltar(antes)`, esperado), e só então a nova o
 *     ganha (`voz.tomar(id, sessao)`).
 *   - Depois do `soltar`, o alvo é conferido de novo: se ele saiu ou caiu no
 *     meio, a troca é cancelada limpa e o foco fica vazio.
 *   - Se o `tomar` falha, o foco fica vazio e a falha é dita.
 *   - A queda do dono fecha o portão na hora e entra na cadeia para o `soltar`.
 * - **Só a primeira entrada do registro toma o foco sozinha.** Um foco vazio
 *   depois de uma queda, de uma saída ou de um ato explícito é da pessoa: uma
 *   entrada nova não o rouba. Conectar, trocar a vista ou outra sessão cair
 *   nunca movem o foco.
 * - O portão também está no transporte de cada sessão (`podeUsarAVoz`): a voz
 *   não sai nem toca fora do foco, ainda que quem captura erre de sessão.
 * - `sessoes()` tem a forma do `SessaoNaTela` do app (`fronteiras.md`, §4).
 *
 * Sem estado de módulo: dois registros não dividem nada. As intercalações estão
 * provadas em `apps/seele-app/bancada/registro-de-sessoes.cjs`.
 *
 * - `abrir`: a abertura de uma sessão (`abrirASessaoDoNavegador`; a bancada a troca);
 * - `instanciar`: a instância própria do `.wasm` de cada sessão;
 * - `voz`: `{ soltar(id), tomar(id, sessao) }`, a captura e a mistura da página
 *   (a ponte as dá); sem ela, só o portão do transporte vale;
 * - `aoMudar({ sessao, motivo })`: o sinal «releia `sessoes()`», com os motivos
 *   do `seele://sessoes` (`entrou`, `mensagens`, `presenca`, `enlace`, `aviso`,
 *   `encerrada`, `foco`, `vista`, `saiu`).
 */
function criarRegistroDeSessoesWeb({
  abrir = abrirASessaoDoNavegador,
  instanciar = instanciarOWasmDaSessao,
  voz = null,
  aoMudar = () => {},
} = {}) {
  // O parâmetro com padrão, num nome declarado: o guarda das chamadas
  // (`no_script_calls_a_function_that_no_script_declares`) lê por linha, e não
  // vê a desestruturação de várias linhas.
  const instanciarOWasm = instanciar;
  let proxima = 1;
  const registros = new Map();
  let frente = 0;
  /** Quem tem o foco agora: o portão de cada transporte lê isto. */
  let foco = 0;
  /** Só a primeira entrada toma o foco sozinha. */
  let primeiraEntrada = true;
  /** A cadeia única do coordenador de voz: trocas, saídas e quedas, na ordem. */
  let cadeia = Promise.resolve();

  /** Põe um passo do foco na cadeia; resolve ou rejeita com ele. */
  function naCadeia(passo) {
    const este = cadeia.catch(() => {}).then(passo);
    cadeia = este.catch(() => {});
    return este;
  }

  /** Se esta sessão ainda está no registro e de pé. */
  const viva = (id) => {
    const registro = registros.get(id);
    return Boolean(registro && registro.sessao && !registro.encerrada && !registro.sessao.acabou);
  };

  const motivoDoEvento = (evento) => {
    if (!evento || typeof evento !== "object") {
      if (evento === "MessagesChanged") return "mensagens";
      if (evento === "RosterChanged") return "presenca";
      if (evento === "TelemetryChanged") return "enlace";
      return "aviso";
    }
    if (evento.Ended) return "encerrada";
    return "aviso";
  };

  function avisar(sessao, motivo) {
    try {
      aoMudar({ sessao, motivo });
    } catch (falha) {
      console.warn("registro: quem ouve as sessões falhou:", falha);
    }
  }

  /**
   * O foco sai de `id` (a queda, a saída): o portão fecha na hora, e o
   * `soltar` entra na cadeia e é esperado.
   */
  function soltarODe(id) {
    if (foco !== id) return naCadeia(async () => {});
    foco = 0;
    return naCadeia(async () => {
      try {
        await voz?.soltar?.(id);
      } catch (falha) {
        console.warn(`registro: soltar a voz da sessão ${id} falhou:`, falha);
      }
      avisar(id, "foco");
    });
  }

  /**
   * Troca o foco de voz, na cadeia: a antiga para antes de a nova começar.
   * Resolve com `{ antes, agora }` (`agora` é `null` quando a troca foi
   * cancelada porque o alvo saiu no meio); rejeita com `NotConnected` para um
   * alvo que não está, e com `{ FalhaAoTomarAVoz }` quando o `tomar` falha.
   */
  function focarVoz(id) {
    const pedido = id ?? 0;
    return naCadeia(async () => {
      const antes = foco;
      if (pedido !== 0 && !viva(pedido)) throw "NotConnected";
      if (antes === pedido) return { antes: antes || null, agora: pedido || null };
      // Ninguém tem o foco enquanto a voz da antiga para.
      foco = 0;
      if (antes !== 0) {
        try {
          await voz?.soltar?.(antes);
        } catch (falha) {
          console.warn(`registro: soltar a voz da sessão ${antes} falhou:`, falha);
        }
      }
      if (pedido === 0) {
        avisar(antes, "foco");
        return { antes: antes || null, agora: null };
      }
      // O alvo pode ter saído ou caído enquanto a antiga soltava.
      if (!viva(pedido)) {
        avisar(antes || pedido, "foco");
        return { antes: antes || null, agora: null, cancelada: true };
      }
      foco = pedido;
      try {
        await voz?.tomar?.(pedido, registros.get(pedido).sessao);
      } catch (falha) {
        if (foco === pedido) foco = 0;
        avisar(pedido, "foco");
        throw { FalhaAoTomarAVoz: { sessao: pedido, motivo: String(falha?.message ?? falha) } };
      }
      // Caiu durante o `tomar`: a queda já fechou o portão e pôs o `soltar` na
      // cadeia, atrás deste passo.
      if (foco !== pedido) return { antes: antes || null, agora: null, cancelada: true };
      avisar(pedido, "foco");
      return { antes: antes || null, agora: pedido };
    });
  }

  /**
   * Entra num servidor: uma sessão nova, à frente. Resolve com `{ id, sessao }`
   * e rejeita como `abrirASessaoDoNavegador`. `emitir({ sessao, geracao,
   * evento })` recebe cada evento carimbado.
   */
  async function entrar({ alvo, apelido, convite = null, aceito = null, emitir = () => {}, aoAcabar = () => {}, cancelamento }) {
    const id = proxima;
    proxima += 1;
    const registro = {
      id,
      alvo,
      sessao: null,
      entrou: Date.now(),
      encerrada: null,
      avisosDeConversa: 0,
      mudou: false,
    };
    registros.set(id, registro);
    let sessao;
    try {
      const wasm = await instanciarOWasm();
      sessao = await abrir({
        wasm,
        alvo,
        apelido,
        convite,
        aceito,
        cancelamento,
        podeUsarAVoz: () => foco === id && registros.get(id) === registro,
        emitir: (evento) => {
          // A sessão que já saiu do registro não fala mais com a casca.
          if (registros.get(id) !== registro) return;
          if (frente !== id) {
            registro.mudou = true;
            if (evento === "MessagesChanged") registro.avisosDeConversa += 1;
          }
          if (evento && evento.Ended) registro.encerrada = evento.Ended.reason;
          emitir({ sessao: id, geracao: registro.sessao?.geracaoDoEnlace ?? 1, evento });
          if (registro.sessao) avisar(id, motivoDoEvento(evento));
        },
        aoAcabar: (motivo) => {
          if (registros.get(id) !== registro) return;
          registro.encerrada = registro.encerrada ?? motivo ?? "LinkLost";
          // O dono que cai deixa o foco vazio, pela cadeia: ninguém o ganha
          // sozinho, e o `soltar` termina antes de qualquer `tomar` seguinte.
          soltarODe(id);
          avisar(id, "encerrada");
          aoAcabar(motivo);
        },
      });
    } catch (motivo) {
      if (registros.get(id) === registro) registros.delete(id);
      throw motivo;
    }
    if (registros.get(id) !== registro) {
      // Saiu do registro enquanto entrava (`encerrar` no meio): é solta.
      await sessao.encerrar();
      throw "NotConnected";
    }
    registro.sessao = sessao;
    frente = id;
    avisar(id, "entrou");
    avisar(id, "vista");
    if (primeiraEntrada) {
      primeiraEntrada = false;
      await focarVoz(id).catch((falha) => console.warn("registro: a primeira sessão não tomou a voz:", falha));
    }
    return { id, sessao };
  }

  /** Traz uma sessão à frente, sem derrubar ninguém. */
  function ver(id) {
    const registro = registros.get(id);
    if (!registro || !registro.sessao) throw "NotConnected";
    frente = id;
    registro.mudou = false;
    registro.avisosDeConversa = 0;
    avisar(id, "vista");
    return { id, sessao: registro.sessao };
  }

  /** Sai de uma sessão (com despedida), ou dispensa uma encerrada. As outras não mudam. */
  async function encerrar(id) {
    const registro = registros.get(id);
    if (!registro) return;
    // Fora do registro primeiro: uma troca pendente para ela se cancela.
    registros.delete(id);
    await soltarODe(id);
    if (frente === id) {
      frente = [...registros.keys()].filter((outro) => registros.get(outro).sessao).pop() ?? 0;
      if (frente) avisar(frente, "vista");
    }
    try {
      await registro.sessao?.encerrar();
    } finally {
      avisar(id, "saiu");
    }
  }

  /** As sessões, na forma do `SessaoNaTela` (`fronteiras.md`, §4). */
  function sessoes() {
    return [...registros.values()]
      .filter((registro) => registro.sessao || registro.encerrada)
      .sort((a, b) => a.entrou - b.entrou || a.id - b.id)
      .map((registro) => {
        let retrato = null;
        try {
          retrato = registro.sessao && !registro.sessao.acabou ? registro.sessao.retrato() : null;
        } catch {
          retrato = null;
        }
        const sala = retrato?.sala ?? {};
        return {
          id: registro.id,
          endereco: registro.alvo.endereco,
          nome: sala.server ?? null,
          a_frente: frente === registro.id,
          foco_de_voz: foco === registro.id,
          na_sala_de_voz: sala.voice_room ?? null,
          // Uma página nunca hospeda: todo servidor dela é de outra pessoa.
          onde: "Outro",
          enlace: registro.encerrada ? "encerrada" : registro.sessao?.voltando ? "reconectando" : "online",
          encerrada: registro.encerrada,
          // Os avisos de conversa que chegaram com a sessão ao fundo: o
          // `MessagesChanged` é grosso, e cada um conta como um.
          nao_lidas: registro.avisosDeConversa,
          mudou: registro.mudou,
        };
      });
  }

  return {
    entrar,
    ver,
    focarVoz,
    encerrar,
    sessoes,
    foco: () => foco || null,
    frente: () => frente || null,
    /** A sessão de um id, ou `null`: o handle por onde saem os comandos dela. */
    sessao: (id) => registros.get(id)?.sessao ?? null,
  };
}
