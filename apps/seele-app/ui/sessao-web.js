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
 * - `transporte(endereco, hashes)`: a WebTransport, trocada na bancada.
 *
 * Rejeita com o `ConnectionError` do desktop.
 */
async function abrirASessaoDoNavegador({
  wasm,
  alvo,
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
  const recusa = ligacao.abrir({
    apelido,
    chave_publica: hexDe(chavePublica),
    impressao: alvo.impressao,
    convite,
    aceito,
  });
  if (recusa) throw recusa.recusa ?? "Unreachable";

  const candidatos = [alvo.endereco, ...(alvo.alternativos ?? [])];
  const etapa = (stage) => emitir({ ConnectStageChanged: { stage } });
  etapa({ Parada: { candidatos: candidatos.length, com_bilhete_e_impressao: false } });

  let wt = null;
  for (const [candidato, onde] of candidatos.entries()) {
    etapa({ Tentando: { candidato, de: candidatos.length, onde, avisou: false } });
    try {
      const tentativa = transporte(onde, alvo.hashes);
      await comPrazo(tentativa.ready, PRAZO_DO_APERTO_MS, "HandshakeTimeout");
      wt = tentativa;
      break;
    } catch (falha) {
      console.warn(`sessão: ${onde} não abriu a WebTransport:`, falha);
    }
  }
  if (!wt) {
    ligacao.fechar();
    etapa({ Desistiu: { motivo: "Unreachable" } });
    throw "Unreachable";
  }

  const sessao = criarASessaoAberta({ ligacao, wt, alvo, emitir, aoAcabar, aoReceberOsHashes });
  try {
    await comPrazo(sessao.aperto, PRAZO_DO_APERTO_MS, "HandshakeTimeout");
  } catch (motivo) {
    etapa({ Desistiu: { motivo } });
    await sessao.encerrar({ calado: true });
    throw motivo;
  }
  etapa("Dentro");
  sessao.comecarAMedir();
  return sessao;
}

/** A sessão sobre uma WebTransport já aberta: o aperto, a leitura e os comandos. */
function criarASessaoAberta({ ligacao, wt, alvo, emitir, aoAcabar, aoReceberOsHashes }) {
  let escritor = null;
  let dentro = false;
  let acabou = false;
  let medindo = null;
  let resolverOAperto;
  let rejeitarOAperto;
  const aperto = new Promise((resolver, rejeitar) => {
    resolverOAperto = resolver;
    rejeitarOAperto = rejeitar;
  });
  /** Quem espera o peso de cada canal (`peso_da_linha`). */
  const pesos = new Map();
  /** Quantas mensagens de um vocabulário mais novo foram puladas. */
  const contas = { desconhecidas: 0, naoSairam: 0 };
  /** Uma escrita por vez no fluxo de controle, na ordem. */
  let escrita = Promise.resolve();

  function escreverOQueEspera() {
    const bytes = ligacao.paraOFio();
    if (bytes.length === 0 || !escritor) return escrita;
    escrita = escrita.then(() => escritor.write(bytes)).catch((falha) => {
      console.warn("sessão: a escrita no fluxo de controle falhou:", falha);
    });
    return escrita;
  }

  async function seguirOsPassos(passos) {
    for (const passo of passos) {
      if (passo === "entrou") {
        dentro = true;
        resolverOAperto();
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
        rejeitarOAperto(passo.recusada);
      } else if (passo.perdida !== undefined) {
        console.warn("sessão: o fluxo de controle se perdeu:", passo.perdida);
        acabar("LinkLost");
      }
    }
    await escreverOQueEspera();
  }

  function chegou(mensagem, eventos) {
    if (mensagem && mensagem.HashesDoNavegador) aoReceberOsHashes(mensagem.HashesDoNavegador.janelas ?? []);
    if (mensagem && mensagem.ChannelWeighed) {
      const peso = mensagem.ChannelWeighed;
      for (const resolver of pesos.get(peso.channel) ?? []) resolver(peso);
      pesos.delete(peso.channel);
    }
    for (const evento of eventos ?? []) {
      emitir(evento);
      if (evento && evento.Ended) acabar(null);
    }
    sessao.aoDobrar?.(mensagem);
  }

  async function ler(leitor) {
    try {
      for (;;) {
        const { value, done } = await leitor.read();
        if (done) break;
        await seguirOsPassos(ligacao.chegou(value));
      }
    } catch (falha) {
      if (!acabou) console.warn("sessão: a leitura do fluxo de controle parou:", falha);
    }
    if (!acabou) await seguirOsPassos(ligacao.fluxoAcabou());
    if (!dentro) rejeitarOAperto("Unreachable");
  }

  /** O fim, uma vez só: a casca ouve o `Ended` (do servidor, ou a queda). */
  function acabar(motivo) {
    if (acabou) return;
    acabou = true;
    if (medindo) clearInterval(medindo);
    for (const [, resolvidos] of pesos) for (const resolver of resolvidos) resolver(null);
    pesos.clear();
    if (dentro && motivo) emitir({ Ended: { reason: motivo } });
    try {
      wt.close();
    } catch {
      // Já fechada.
    }
    ligacao.fechar();
    if (dentro) aoAcabar(motivo);
  }

  (async () => {
    try {
      const fluxo = await wt.createBidirectionalStream();
      escritor = fluxo.writable.getWriter();
      escreverOQueEspera();
      ler(fluxo.readable.getReader());
    } catch (falha) {
      console.warn("sessão: o fluxo de controle não abriu:", falha);
      rejeitarOAperto("Unreachable");
    }
  })();
  wt.closed.then(
    () => acabar("LinkLost"),
    () => acabar("LinkLost"),
  );

  /** Os datagramas da voz, no contrato do `voz-web.js`. */
  let ouvinteDaVoz = null;
  let escritorDaVoz = null;
  (async () => {
    try {
      const leitor = wt.datagrams.readable.getReader();
      for (;;) {
        const { value, done } = await leitor.read();
        if (done) break;
        ouvinteDaVoz?.(value);
      }
    } catch {
      // A conexão acabou; o fim é dito por `acabar`.
    }
  })();
  const transporteDaVoz = Object.freeze({
    enviar(bytes) {
      if (acabou) return false;
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

  const sessao = {
    aperto,
    contas,
    transporteDaVoz,
    get acabou() {
      return acabou;
    },
    /** Quem a ponte põe para ouvir cada mensagem dobrada (a voz, a tela). */
    aoDobrar: null,
    /** O que esta ponta mede, para o retrato; a ponte o troca. */
    medidas: () => ({ jitterMs: 0, perda: 0 }),
    /** O que a ponte faz a cada volta da medida (os volumes, a voz). */
    aCadaVolta: null,
    comecarAMedir() {
      medindo = setInterval(async () => {
        const { jitterMs, perda } = sessao.medidas();
        if (ligacao.medir(await idaEVolta(), jitterMs, perda)) emitir("TelemetryChanged");
        sessao.aCadaVolta?.();
      }, VOLTA_DA_MEDIDA_MS);
    },
    /** O ganho de cada voz com volume escolhido, pela alça da mistura. */
    ganhosDaVoz: () => (acabou ? [] : ligacao.ganhosDaVoz()),
    /** O retrato do `.wasm`: a sala, os contados e a telemetria. */
    retrato: () => ligacao.retrato(),
    /**
     * Um comando da sessão: `{ resposta }`, `{ erro }`, ou `null` quando ele
     * não é da sessão. Os eventos saem para a casca antes de a resposta voltar.
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
    /** Espera o `ChannelWeighed` de um canal; `null` se a sessão acabar antes. */
    esperarOPeso(canal) {
      return new Promise((resolver) => {
        if (acabou) return resolver(null);
        const lista = pesos.get(canal) ?? [];
        lista.push(resolver);
        pesos.set(canal, lista);
      });
    },
    /**
     * Fecha a sessão. `calado` é a que nunca abriu (a recusa, o prazo do
     * aperto): sem `Ended`, porque a casca nunca a viu.
     */
    async encerrar({ calado = false } = {}) {
      if (!acabou && escritor) {
        await escreverOQueEspera();
        try {
          await comPrazo(escritor.close(), ESPERA_DA_RECUSA_MS, "prazo");
        } catch {
          // A recusa que não chegou no prazo: largamos mesmo assim.
        }
      }
      acabar(calado ? null : null);
    },
  };
  return sessao;
}
