// O enlace WebRTC da página (0.17; contrato da 0.17, D2, D3, D4, D7, D8 e D9).
//
// A página só **liga**: o anfitrião é sempre um nativo, ICE completo e DTLS
// passivo, e não há troca de SDP. A página sorteia a credencial ICE U
// (`ufrag = pwd = U`), escreve a própria oferta com ela e **sintetiza** a
// descrição do anfitrião a partir do convite. É o desenho do libp2p
// webrtc-direct.
//
// # O que confere quem atendeu
//
// O `a=fingerprint` da descrição do anfitrião é o `fp=` do convite, escrito
// pela página e nunca recebido de ninguém (D3). O navegador recusa o DTLS com
// outro certificado antes de qualquer canal abrir. Por isso o sinal, o ponto e
// a rede podem ser hostis: o pior que fazem é a página não entrar. O
// certificado DTLS da página é efêmero, um por conexão, e a impressão dele vai
// na prova D5, assinada pela chave da pessoa (`seele_web::ligacao`).
//
// # O que mora aqui, e o que não mora
//
// **Nenhuma linha do protocolo da sessão**: os bytes do controle, dos fluxos e
// da voz são opacos, e quem os enquadra é o `.wasm`. Mora aqui:
// - o envelope dos canais (D4): `tag ‖ corpo`, com 0x00 dados, 0x01 fim,
//   0x02 corte e 0x03 apresentação;
// - o SDP sintetizado;
// - a conversa com o ponto (`POST /sinal/1`). As linhas são montadas e lidas
//   pelo `.wasm` (`sinal_chama`, `sinal_resposta`);
// - a contrapressão dos canais de volume e a classificação da falha.
//
// # A forma
//
// `criarEnlaceRtc` devolve um objeto com a forma que a sessão consumia da
// WebTransport: `ready`, `closed`, `close()`, `controle()`,
// `createUnidirectionalStream()`, `createBidirectionalStream()`,
// `incomingUnidirectionalStreams`, `canalDaVoz` e `getStats()`. Assim a
// `sessao-web.js` trocou de transporte sem trocar de lógica.
//
// **Sem estado de módulo.** Cada enlace tem o seu `RTCPeerConnection`, os seus
// canais, a sua fila e o seu relógio; várias sessões abrem vários enlaces ao
// mesmo tempo (E3).

/** O protocolo DCEP dos canais extras (D4; `seele_proto::transport`). */
const PROTOCOLO_DO_CANAL = "seele/2";

/** As etiquetas do envelope (D4; `enlace-api.md`). */
const ENVELOPE = Object.freeze({ dados: 0x00, fim: 0x01, corte: 0x02, apresentacao: 0x03, credito: 0x04 });

/** O maior corpo de um 0x00. */
const MAX_DADOS = 16 * 1024;

/** A maior mensagem de voz, sem envelope. */
const MAX_VOZ = 1100;

/** A maior apresentação (0x03). */
const MAX_APRESENTACAO = 512;

/**
 * A fila de entrada de um canal: quem passa disto é cortado (código 2). É
 * também a janela do crédito (0x04): quem lê anuncia «consumidos + isto».
 */
const MAX_FILA_DE_ENTRADA = 1024 * 1024;

/**
 * **O crédito de recepção somado por conexão** (0x04, a contrapressão): o que
 * a página aceita ter esperando leitura em todos os canais de volume que ela
 * lê, juntos. Um canal só tem até a janela inteira ([`MAX_FILA_DE_ENTRADA`]);
 * oito deles ao mesmo tempo, e o nono espera um outro consumir. O controle fica
 * fora desta soma: ele nunca espera um anexo para andar.
 */
const MAX_CREDITO_DA_CONEXAO = 8 * 1024 * 1024;

/**
 * **O limiar baixo global dos canais de volume** (R4 da revisão): os canais
 * "u" e "b" só escrevem enquanto a soma do que está esperando em todos eles
 * fica abaixo disto. O controle e a voz saem na hora. Com uma associação SCTP
 * só, uma rajada de anexo acima disto atrasaria a voz atrás dela.
 */
const LIMIAR_DO_VOLUME = 64 * 1024;

/** De quanto em quanto tempo a fila do volume confere a soma, quando espera. */
const VOLTA_DO_VOLUME_MS = 10;

/** O prazo do aperto: do discar até o controle, a voz e a apresentação. */
const PRAZO_DO_ENLACE_MS = 20_000;

/** Quanto o caminho direto tem, antes de a página chamar pelo ponto (D9). */
const ESPERA_DO_DIRETO_MS = 1_500;

/** Quanto a página espera o próprio candidato refletido pelo STUN do ponto. */
const ESPERA_DO_REFLETIDO_MS = 1_500;

/**
 * Quantos candidatos refletidos (`srflx`) a chamada pelo ponto leva: o teto do
 * protocolo, `LIMITE_DE_CANDIDATOS = 2` (`seele-proto/src/encontro.rs:1277`). O
 * ponto recusa o `CHAMA` inteiro com mais que isto, ou com um repetido.
 */
const LIMITE_DE_SRFLX = 2;

// O anfitrião admite até três candidatos distintos por credencial e não
// renova o prazo do par ao receber outro CHAMADO. Atualizações são precoces,
// sequenciais e limitadas; não são um laço de reconexão.
const LIMITE_DE_ANUNCIADOS = 3;
const LIMITE_DE_CHAMADAS_AO_PONTO = 3;
const JANELA_DE_ATUALIZACAO_MS = 8_000;

/**
 * Quanto a página espera o `getStats()` que diz por qual par o ICE abriu. É
 * telemetria, **best effort**: com a conexão já aberta, ela não pode segurar o
 * `ready`. Sem resposta no prazo, o `ready` assenta sem o candidato vencedor
 * (`null`: «não sei»), e a volta usa os alvos do convite.
 */
const ESPERA_DO_PAR_VENCEDOR_MS = 1_000;

/** O prazo do `POST /sinal/1`: o ponto espera o `ATENDE` por 3 s. */
const PRAZO_DO_SINAL_MS = 5_000;

/** Quanto um «u» desta página espera a confirmação (0x01) de quem lê. */
const PRAZO_DA_ENTREGA_MS = 30_000;

/** O keepalive: 5 s sem nada saindo, um 0x00 pela voz (`enlace-api.md`). */
const KEEPALIVE_MS = 5_000;

/** A porta padrão de um ponto de encontro (`seele_proto::encontro::PORTA_PADRAO`). */
const PORTA_DO_PONTO = 8384;

/** Os códigos de corte do enlace (`enlace-api.md`). */
const CORTE = Object.freeze({ violacao: 1, fila: 2, desconhecido: 3 });

/**
 * **As falhas do enlace**, cada uma com a frase dela no bloco do transporte da
 * `frases.js` (D8). Nenhuma promete conectividade.
 */
const FALHA_DO_ENLACE = Object.freeze({
  /**
   * O ICE abriu e a conexão segura (DTLS) falhou. **Não prova impressão
   * diferente**: o navegador não diz o motivo (sem `errorDetail`; spike C).
   * Os motivos possíveis são três, e a frase os lista sem afirmar nenhum: o
   * servidor não é o do link, a chave dele mudou, ou a conexão segura falhou.
   */
  verificacaoFalhou: "VerificacaoDaConexaoSegura",
  /** Nenhum candidato respondeu, e não havia ponto para chamar. */
  semCaminho: "SemCaminho",
  /** Sem ponto, e os alvos do link eram todos nomes (DDNS): nada respondeu neles. */
  nomeSemResposta: "NomeSemResposta",
  /** Havia ponto, mas o STUN dele não respondeu: a rede bloqueia UDP. */
  semRefletido: "SemRefletido",
  /** O ponto disse que ninguém mora nas marcas do convite. */
  ninguemNoPonto: "NinguemNoPonto",
  /** O anfitrião mora no ponto e não atendeu a chamada. */
  anfitriaoCalado: "AnfitriaoCalado",
  /** O ponto recusou a chamada (taxa ou cheio). */
  pontoRecusou: "PontoRecusou",
  /** O ponto não respondeu ao sinal (fora do ar, ou sem HTTPS). */
  sinalMudo: "SinalMudo",
  /** Tudo respondeu, e o caminho direto não abriu: NAT simétrico, provavelmente. */
  caminhoNaoAbriu: "CaminhoNaoAbriu",
  /** O aperto passou do prazo por outro motivo. */
  prazo: "HandshakeTimeout",
  /** O anfitrião falou fora do envelope. */
  protocolo: "ProtocoloDoEnlace",
  /** Este navegador não tem WebRTC (ou a página o perdeu). */
  semWebRtc: "SemWebRtc",
});

// **Limpeza única da rodada de homologação da 0.17.** A chave `seele.turn` só existiu
// nela, guardando a credencial temporária de um relé de teste, e o SEELE não tem
// relé: é código na máquina de cada um, e o VPS é só o ponto de encontro. Apaga só
// essa chave (nunca `clear()`, nunca outra), uma vez, na carga da página.
try { localStorage.removeItem("seele.turn"); } catch { /* armazenamento negado: não há o que apagar */ }

/** Um U novo: 32 de `[A-Za-z0-9]`, sem viés (D2). */
function sortearCredencialIce() {
  const alfabeto = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  const saida = [];
  while (saida.length < 32) {
    for (const byte of crypto.getRandomValues(new Uint8Array(48))) {
      // 248 = 4 × 62: o que passa disso é descartado, para cada letra valer igual.
      if (byte < 248 && saida.length < 32) saida.push(alfabeto[byte % 62]);
    }
  }
  return saida.join("");
}

/** A impressão em hex (64) na escrita do SDP: `AA:BB:…`. */
function impressaoNoSdp(hex) {
  return hex.toUpperCase().match(/../g).join(":");
}

/** A impressão do SDP (`AA:BB:…`) em hex minúsculo, ou `null`. */
function impressaoDoSdp(texto) {
  const limpo = String(texto ?? "").replace(/:/g, "").toLowerCase();
  return /^[0-9a-f]{64}$/.test(limpo) ? limpo : null;
}

/**
 * Um alvo do convite (`host:porta`, `[v6]:porta` ou `host`) separado, para virar
 * candidato. `null` para o que não se separa.
 */
function separarAlvo(texto, portaPadrao) {
  const alvo = String(texto ?? "").trim();
  const seis = alvo.match(/^\[([0-9A-Fa-f:.]+)\](?::(\d{1,5}))?$/);
  if (seis) return { maquina: seis[1], porta: Number(seis[2] ?? portaPadrao), familia: 6 };
  const resto = alvo.match(/^([^:\s\[\]]+)(?::(\d{1,5}))?$/);
  if (!resto) return null;
  const porta = Number(resto[2] ?? portaPadrao);
  if (!(porta > 0 && porta < 65536)) return null;
  const ehQuatro = /^\d{1,3}(\.\d{1,3}){3}$/.test(resto[1]);
  return { maquina: resto[1], porta, familia: ehQuatro ? 4 : 0 };
}

/**
 * `máquina:porta` de um candidato que o navegador contou, na forma que o sinal
 * lê: o IPv6 leva colchetes (`[2a02:…:6]:58773`). Sem eles o `SocketAddr` do
 * `sinal_chama` não lê o texto, recusa o candidato, e a chamada inteira morre
 * com `{"erro": "candidato"}`, o IPv4 junto.
 */
function escreverEndereco(maquina, porta) {
  const texto = String(maquina ?? "");
  const seis = texto.includes(":") && !texto.startsWith("[");
  return `${seis ? `[${texto}]` : texto}:${porta}`;
}

/**
 * A linha `candidate:` de um candidato do anfitrião. Um nome (o DDNS do
 * convite) vai como nome: o navegador o resolve ou o descarta, e o resultado é
 * medido em `estado-browser.md`.
 */
function linhaDoCandidato(indice, { maquina, porta }) {
  // A prioridade cai com a ordem do convite: o primeiro alvo é o da rede local.
  const prioridade = 2130706431 - indice;
  return `candidate:${indice + 1} 1 udp ${prioridade} ${maquina} ${porta} typ host`;
}

/**
 * **A descrição do anfitrião, sintetizada** (D2), na forma que o spike C
 * (24/10/2026) mediu no Chromium 149 e no WebKit 26.5: DTLS passivo, o `fp` do
 * convite, a mesma credencial da oferta, o `mid` e o `BUNDLE` dela, as linhas
 * `a=candidate:` dadas e, quando a lista acabou, `a=end-of-candidates`.
 *
 * **Sem `a=ice-lite`.** O spike C mediu o anfitrião lite; agora o anfitrião nativo
 * é ICE completo (`seele-enlace/src/ponta.rs`): ele checa os candidatos e aprende
 * o peer-reflexive, o endereço que o NAT do celular deu e que nenhum candidato
 * declarou. A página segue a controladora (ela é quem oferece).
 */
function descricaoDoAnfitriao({ mid, credencial, impressao, candidatos = [], acabou = false }) {
  return [
    "v=0",
    "o=- 1 1 IN IP4 0.0.0.0",
    "s=-",
    "t=0 0",
    `a=group:BUNDLE ${mid}`,
    "m=application 9 UDP/DTLS/SCTP webrtc-datachannel",
    "c=IN IP4 0.0.0.0",
    `a=mid:${mid}`,
    `a=ice-ufrag:${credencial}`,
    `a=ice-pwd:${credencial}`,
    `a=fingerprint:sha-256 ${impressaoNoSdp(impressao)}`,
    "a=setup:passive",
    "a=sctp-port:5000",
    `a=max-message-size:${MAX_DADOS + 1}`,
    ...candidatos,
    ...(acabou ? ["a=end-of-candidates"] : []),
    "",
  ].join("\r\n");
}

/** A oferta da página com a credencial U nos dois campos (D2). */
function ofertaComCredencial(sdp, credencial) {
  return sdp
    .replace(/a=ice-ufrag:[^\r\n]*/g, `a=ice-ufrag:${credencial}`)
    .replace(/a=ice-pwd:[^\r\n]*/g, `a=ice-pwd:${credencial}`);
}

/**
 * Os servidores ICE desta tentativa (D8): o STUN do ponto do convite, e só ele.
 * Nenhum padrão de terceiro: um STUN público aprenderia quem liga para quem.
 */
function servidoresIce({ ponto = null } = {}) {
  const lista = [];
  const separado = ponto ? separarAlvo(ponto, PORTA_DO_PONTO) : null;
  if (separado) {
    const maquina = separado.familia === 6 ? `[${separado.maquina}]` : separado.maquina;
    lista.push({ urls: `stun:${maquina}:${separado.porta}` });
  }
  return lista;
}

/**
 * O endereço do sinal de um ponto: `https://<máquina do ponto>/sinal/1`. O
 * ponto fala UDP na 8384; o sinal é um HTTPS no mesmo nome, na 443 (um proxy
 * TLS na frente do `seele-encontro --sinal`). Um ponto escrito como IP não tem
 * certificado que o navegador aceite: sem sinal, só o STUN.
 */
function urlDoSinalDoPonto(ponto) {
  const separado = separarAlvo(ponto, PORTA_DO_PONTO);
  if (!separado || separado.familia !== 0) return null;
  return `https://${separado.maquina}/sinal/1`;
}

/** Bytes com a etiqueta do envelope na frente. */
function envelopar(tag, corpo) {
  const bytes = new Uint8Array(1 + (corpo ? corpo.byteLength : 0));
  bytes[0] = tag;
  if (corpo) bytes.set(corpo, 1);
  return bytes;
}

/** O corte: 0x02 e o código em u32 big-endian. */
function envelopeDoCorte(codigo) {
  const corpo = new Uint8Array(4);
  new DataView(corpo.buffer).setUint32(0, codigo >>> 0, false);
  return envelopar(ENVELOPE.corte, corpo);
}

/** Lê uma mensagem do envelope: `{ tag, corpo }`, ou `null` se não for uma. */
function lerEnvelope(dados) {
  const bytes = dados instanceof Uint8Array ? dados : new Uint8Array(dados);
  if (bytes.length === 0) return null;
  const tag = bytes[0];
  const corpo = bytes.subarray(1);
  if (tag === ENVELOPE.dados && corpo.length >= 1 && corpo.length <= MAX_DADOS) return { tag, corpo };
  if (tag === ENVELOPE.fim && corpo.length === 0) return { tag, corpo };
  if (tag === ENVELOPE.corte && corpo.length === 4) {
    return { tag, corpo, codigo: new DataView(corpo.buffer, corpo.byteOffset, 4).getUint32(0, false) };
  }
  if (tag === ENVELOPE.apresentacao && corpo.length <= MAX_APRESENTACAO) return { tag, corpo };
  if (tag === ENVELOPE.credito && corpo.length === 8) {
    // u64 big-endian. Acima de 2^53 o número perde precisão, e é tanto crédito
    // que a página nunca chega lá: fica no maior inteiro seguro.
    const vista = new DataView(corpo.buffer, corpo.byteOffset, 8);
    const limite = vista.getUint32(0, false) * 2 ** 32 + vista.getUint32(4, false);
    return { tag, corpo, limite: Math.min(limite, Number.MAX_SAFE_INTEGER) };
  }
  return null;
}

/** O crédito: 0x04 e o limite absoluto em u64 big-endian. */
function envelopeDoCredito(limite) {
  const corpo = new Uint8Array(8);
  const vista = new DataView(corpo.buffer);
  vista.setUint32(0, Math.floor(limite / 2 ** 32), false);
  vista.setUint32(4, limite % 2 ** 32, false);
  return envelopar(ENVELOPE.credito, corpo);
}

/**
 * **O crédito do anfitrião, do lado de quem escreve** (0x04, Δ8 da linha 2): o
 * total de bytes de dados que a página pode ter mandado neste canal desde a
 * abertura. **Começa em 0**: a linha 2 exige o 0x04, e nenhum dado sai antes do
 * primeiro anúncio do par (sem rajada antes da cota). Um pedaço só sai se
 * `enviados + pedaço <= limite`; sem crédito, quem escreve espera, e não corta.
 * Apresentação, fim, corte e o próprio crédito nunca esperam.
 */
function criarCreditoDoPar(canal, contas = null) {
  let limite = 0;
  let enviados = 0;
  let morto = null;
  const esperas = [];
  const cabe = (bytes) => enviados + bytes <= limite;
  function andar() {
    while (esperas.length > 0 && cabe(esperas[0].bytes)) esperas.shift().resolver();
  }
  function fechar(motivo) {
    if (morto !== null) return;
    morto = motivo ?? "caiu";
    for (const uma of esperas.splice(0)) uma.rejeitar(morto);
  }
  canal.addEventListener("close", () => fechar("caiu"));
  return {
    /** O anúncio do par: absoluto, e só para cima. */
    subir(novo) {
      if (contas) contas.creditosRecebidos += 1;
      if (novo > limite) limite = novo;
      andar();
    },
    /** Espera crédito para `bytes` de dados. */
    esperar(bytes) {
      if (morto !== null) return Promise.reject(morto);
      if (esperas.length === 0 && cabe(bytes)) return Promise.resolve();
      if (contas) contas.esperasPorCredito += 1;
      return new Promise((resolver, rejeitar) => esperas.push({ bytes, resolver, rejeitar }));
    },
    /** Os bytes de dados que saíram. */
    contar(bytes) {
      enviados += bytes;
    },
    fechar,
    get limite() {
      return limite;
    },
    get enviados() {
      return enviados;
    },
  };
}

/**
 * **O crédito que a página concede, por conexão** (0x04, do lado de quem lê).
 * Cada canal que a página lê anuncia ao abrir e, depois de consumir um quarto
 * da janela, anuncia de novo «consumidos + janela» — o mesmo ritmo do
 * `Recebe::consumiu` do enlace nativo. Absoluto, e não incremental: uma corrida
 * entre o anúncio e os dados nunca conta o mesmo byte duas vezes.
 *
 * O que ainda pode chegar e o que espera leitura (`limite - consumidos`) é
 * somado sobre os canais de volume, até [`MAX_CREDITO_DA_CONEXAO`]: um canal
 * cujo leitor parou segura só a janela dele, e os outros andam com o resto. Um
 * canal faminto (menos que um pedaço de dados de folga) é revisto toda vez que
 * outro consome ou sai, para ninguém esperar um crédito que nunca vem.
 *
 * **O fim do par não esvazia a fila** (o defeito que o Codex mediu em 24/10:
 * nove canais com o fim e sem leitor liberavam a soma, e os leitores depois
 * devolviam 9 437 184 B acima do teto). Com o fim, a cota que o par não usou
 * volta na hora (`limite - recebidos`), mas o que espera leitura continua
 * contado até ser **consumido**; o canal sai da soma quando a fila esvazia, ou
 * quando ela é **descartada** de fato (o leitor cancelou, o canal caiu, o par
 * cortou ou violou).
 */
function criarCreditoDaConexao({ total = MAX_CREDITO_DA_CONEXAO, janela = MAX_FILA_DE_ENTRADA, contas = null } = {}) {
  const canais = new Set();
  const passo = Math.floor(janela / 4);
  // Antes do fim: o concedido e não consumido. Depois do fim: só o que chegou e
  // espera leitura (o par não manda mais nada).
  const emAbertoDe = (um) => (um.fim ? um.recebidos - um.consumidos : um.limite - um.consumidos);
  const emAberto = () => {
    let soma = 0;
    for (const um of canais) if (!um.fixo) soma += emAbertoDe(um);
    return soma;
  };
  function sair(um) {
    if (!canais.has(um)) return;
    canais.delete(um);
    reverTodos();
  }
  function rever(um) {
    if (um.fim || !canais.has(um) || um.canal.readyState !== "open") return;
    const desejado = um.consumidos + janela;
    let novo = desejado;
    if (!um.fixo) novo = Math.min(desejado, um.limite + Math.max(0, total - emAberto()));
    if (novo <= um.limite && um.anunciado) return;
    const faminto = um.limite - um.consumidos < MAX_DADOS && novo - um.consumidos >= MAX_DADOS;
    if (um.anunciado && novo - um.limite < passo && !faminto) return;
    try {
      um.canal.send(envelopeDoCredito(novo));
    } catch {
      return;
    }
    um.limite = novo;
    um.anunciado = true;
    if (contas) contas.creditosAnunciados += 1;
  }
  const reverTodos = () => {
    for (const um of canais) rever(um);
  };
  return {
    /**
     * Um canal que a página lê. `fixo`: fora da soma (o controle). Devolve
     * `{ anunciar(), chegaram(bytes), consumiu(bytes), fimDoPar(), descartou() }`.
     */
    abrir(canal, { fixo = false } = {}) {
      const um = { canal, fixo, limite: 0, recebidos: 0, consumidos: 0, anunciado: false, fim: false };
      canais.add(um);
      const drenou = () => um.fim && um.consumidos >= um.recebidos;
      return {
        anunciar: () => rever(um),
        /** Bytes de dados que entraram na fila deste canal. */
        chegaram(bytes) {
          um.recebidos += bytes;
        },
        /** Bytes que o leitor tirou da fila. */
        consumiu(bytes) {
          um.consumidos += bytes;
          if (drenou()) {
            sair(um);
            return;
          }
          rever(um);
          if (!fixo) reverTodos();
        },
        /**
         * O fim do par: a cota que ele não usou volta para os outros; o que
         * espera leitura continua contado até ser consumido.
         */
        fimDoPar() {
          if (um.fim) return;
          um.fim = true;
          if (drenou()) sair(um);
          else reverTodos();
        },
        /** A fila foi jogada fora (cancelada, caída, cortada): sai da soma. */
        descartou: () => sair(um),
      };
    },
    /** O que está concedido e não consumido, na soma dos canais de volume. */
    emAberto,
  };
}

/**
 * Gera o certificado DTLS desta conexão (ECDSA P-256, efêmero) e a impressão
 * SHA-256 dele em hex, a que vai na prova D5.
 */
async function prepararCertificadoDtls() {
  if (typeof RTCPeerConnection !== "function" || typeof RTCPeerConnection.generateCertificate !== "function") {
    throw FALHA_DO_ENLACE.semWebRtc;
  }
  const certificado = await RTCPeerConnection.generateCertificate({ name: "ECDSA", namedCurve: "P-256" });
  const impressao = (certificado.getFingerprints?.() ?? [])
    .filter((uma) => String(uma.algorithm).toLowerCase() === "sha-256")
    .map((uma) => impressaoDoSdp(uma.value))
    .find(Boolean);
  if (!impressao) throw FALHA_DO_ENLACE.semWebRtc;
  return { certificado, impressao };
}

/**
 * A fila de volume de um enlace: a vez de cada canal "u" e "b" escrever um
 * pedaço, por ordem de chegada (round-robin entre quem espera), enquanto **a
 * soma do que espera no SCTP mais o que já foi reservado** fica abaixo de
 * [`LIMIAR_DO_VOLUME`].
 *
 * **A vez é uma reserva** (o defeito que o Codex mediu em 24/10: sem ela, 16
 * canais passavam juntos e a soma chegava a 262 160 com o limiar de 65 536).
 * `vez(bytes, canal)` só resolve quando há crédito para aqueles bytes, e já os
 * desconta; quem recebe a reserva manda o pedaço e a devolve
 * (`reserva.devolver()`), mandando ou não: no envio o crédito passa a ser
 * contado no `bufferedAmount` do canal; no erro, só volta. A liberação solta
 * um de cada vez, e só enquanto cabe. Um canal que sai (`largar`) rejeita
 * quem dele esperava; a fila fechada rejeita quem espera e quem chega.
 */
function criarAFilaDoVolume(limiar = LIMIAR_DO_VOLUME) {
  const canais = new Set();
  const esperando = [];
  let reservado = 0;
  let fechada = null;
  let relogio = null;
  const soma = () => {
    let total = 0;
    for (const canal of canais) total += canal.bufferedAmount;
    return total;
  };
  /** Se `bytes` cabem agora; um pedaço maior que o limiar passa sozinho. */
  const cabe = (bytes) => soma() + reservado + bytes <= limiar || (soma() === 0 && reservado === 0);
  function reservar(bytes) {
    reservado += bytes;
    let devolvida = false;
    return {
      devolver() {
        if (devolvida) return;
        devolvida = true;
        reservado = Math.max(0, reservado - bytes);
        andar();
      },
    };
  }
  function andar() {
    if (relogio) clearTimeout(relogio);
    relogio = null;
    if (fechada !== null) return;
    while (esperando.length > 0 && cabe(esperando[0].bytes)) {
      const um = esperando.shift();
      um.resolver(reservar(um.bytes));
    }
    if (esperando.length > 0) relogio = setTimeout(andar, VOLTA_DO_VOLUME_MS);
  }
  return {
    soma,
    /** O que está reservado e ainda não foi mandado (para a medida). */
    reservado: () => reservado,
    juntar(canal) {
      canais.add(canal);
      canal.bufferedAmountLowThreshold = Math.floor(limiar / 4);
      canal.addEventListener("bufferedamountlow", andar);
    },
    /** O canal saiu: quem dele esperava é rejeitado, e o crédito dele volta. */
    largar(canal) {
      canais.delete(canal);
      for (let i = esperando.length - 1; i >= 0; i -= 1) {
        if (esperando[i].canal === canal) esperando.splice(i, 1)[0].rejeitar("caiu");
      }
      andar();
    },
    /** Espera a vez de escrever um pedaço de `bytes` em `canal`: a reserva. */
    vez(bytes, canal = null) {
      if (fechada !== null) return Promise.reject(fechada);
      if (esperando.length === 0 && cabe(bytes)) return Promise.resolve(reservar(bytes));
      return new Promise((resolver, rejeitar) => {
        esperando.push({ bytes, canal, resolver, rejeitar });
        if (!relogio) relogio = setTimeout(andar, VOLTA_DO_VOLUME_MS);
      });
    },
    /** O enlace fechou: ninguém mais espera, nem chega. */
    fechar(motivo) {
      fechada = motivo ?? "caiu";
      if (relogio) clearTimeout(relogio);
      relogio = null;
      for (const um of esperando.splice(0)) um.rejeitar(fechada);
    },
  };
}

/** Espera o canal esvaziar (para o fim não ser cortado pelo `close`). */
function esvaziar(canal) {
  if (canal.readyState !== "open" || canal.bufferedAmount === 0) return Promise.resolve();
  return new Promise((resolver) => {
    const ver = () => {
      if (canal.readyState !== "open" || canal.bufferedAmount === 0) resolver();
      else setTimeout(ver, VOLTA_DO_VOLUME_MS);
    };
    ver();
  });
}

/**
 * O lado que lê de um canal com envelope: um `ReadableStream` de bytes. O fim
 * fecha, o corte erra com `{ corte: código }`, e o canal que cai sem fim erra
 * com `"caiu"`. A fila de entrada tem teto: quem passa é cortado (código 2).
 */
function leituraDoCanal(canal, { aoCortarPorFila, devolverOFim = false, janela = null, aoCredito = null, controle = false } = {}) {
  let controlador = null;
  // `acabou`: o par não manda mais nada (o fim, o corte, a queda); `fimPendente`:
  // o fim chegou com dados ainda na fila, e o fluxo fecha quando ela esvaziar.
  let acabou = false;
  let fimPendente = false;
  let descartado = false;
  // **O que quem lê consumiu**, para o crédito (0x04): o que entrou na fila
  // menos o que ainda espera nela. O `pull` é chamado a cada leitura (a marca
  // d'água é a janela), e é nele que a página vê o leitor andar. Por isso o fim
  // do par não fecha o fluxo na hora: com o fluxo fechado o `pull` não vem mais,
  // e o que fosse lido depois nunca seria contado como consumido.
  let enfileirados = 0;
  let consumidosVistos = 0;
  const naFila = () => Math.max(0, MAX_FILA_DE_ENTRADA - (controlador?.desiredSize ?? MAX_FILA_DE_ENTRADA));
  function medir() {
    if (descartado || !janela) return;
    const consumidos = enfileirados - naFila();
    if (consumidos > consumidosVistos) {
      janela.consumiu(consumidos - consumidosVistos);
      consumidosVistos = consumidos;
    }
  }
  function fecharSeEsvaziou() {
    if (fimPendente && naFila() === 0) {
      fimPendente = false;
      controlador.close();
    }
  }
  /** A fila foi jogada fora: o fluxo erra, e o canal sai da soma da conexão. */
  function descartar(motivo) {
    acabou = true;
    fimPendente = false;
    descartado = true;
    janela?.descartou();
    controlador.error(motivo);
  }
  const leitura = new ReadableStream(
    {
      start(c) {
        controlador = c;
      },
      pull() {
        medir();
        fecharSeEsvaziou();
      },
      cancel() {
        acabou = true;
        fimPendente = false;
        descartado = true;
        janela?.descartou();
        try {
          if (canal.readyState !== "open") return;
          if (controle) {
            // **Largar a leitura do controle não fecha a conexão** (Δ8, o A em
            // 23:05): sem corte no fio, porque o 0x02 no `ctl` é o fecho. O que
            // chegar é jogado fora, e o crédito fica sem fim, para quem escreve
            // nunca ficar preso esperando um leitor que saiu.
            canal.send(envelopeDoCredito(Number.MAX_SAFE_INTEGER));
            return;
          }
          // O STOP do QUIC: o leitor não quer mais, e diz ao par.
          canal.send(envelopeDoCorte(0));
        } catch {
          // Já fechado.
        }
      },
    },
    // Contada em bytes, com a janela por marca: `janela - desiredSize` é
    // quanto espera na fila.
    new ByteLengthQueuingStrategy({ highWaterMark: MAX_FILA_DE_ENTRADA }),
  );
  function chegou(dados) {
    const lido = lerEnvelope(dados);
    // O crédito do par é de quem escreve neste canal (o "b" e o controle), e
    // não entra no fluxo. Ele vale mesmo depois de a leitura acabar (o fim do
    // par num "b", o leitor que largou o controle): quem escreve continua. Num
    // canal que a página só lê, ele não tem sentido, e fica sem efeito, como no
    // enlace nativo.
    if (lido && lido.tag === ENVELOPE.credito) {
      aoCredito?.(lido.limite);
      return;
    }
    if (acabou) return;
    if (!lido || lido.tag === ENVELOPE.apresentacao) {
      descartar("protocolo");
      try {
        canal.send(envelopeDoCorte(CORTE.violacao));
      } catch {
        // Já fechado.
      }
      return;
    }
    if (lido.tag === ENVELOPE.dados) {
      // Com o crédito, quem escreve nunca passa da janela; quem passa é quem
      // não o respeita, e é cortado com o código da fila, dito.
      if (naFila() + lido.corpo.length > MAX_FILA_DE_ENTRADA) {
        descartar({ corte: CORTE.fila });
        aoCortarPorFila?.();
        try {
          canal.send(envelopeDoCorte(CORTE.fila));
        } catch {
          // Já fechado.
        }
        return;
      }
      enfileirados += lido.corpo.length;
      janela?.chegaram(lido.corpo.length);
      controlador.enqueue(lido.corpo.slice());
    } else if (lido.tag === ENVELOPE.fim) {
      // O fim não esvazia a fila: o fluxo fecha quando o leitor tirar o resto.
      acabou = true;
      fimPendente = true;
      janela?.fimDoPar();
      fecharSeEsvaziou();
      // **A entrega** (`enlace-api.md`): quem lê um «u» devolve o 0x01 ao
      // receber o 0x01, e é por ele que quem escreveu sabe que tudo chegou.
      if (devolverOFim) {
        try {
          if (canal.readyState === "open") canal.send(envelopar(ENVELOPE.fim));
        } catch {
          // Já fechado.
        }
      }
    } else if (lido.tag === ENVELOPE.corte) {
      descartar({ corte: lido.codigo });
    }
  }
  /** O canal fechou: sem o fim do par, o que veio não está inteiro, e é jogado fora. */
  function caiu() {
    if (acabou) return;
    descartar("caiu");
  }
  return {
    leitura,
    chegou,
    caiu,
    get acabou() {
      return acabou;
    },
  };
}

/**
 * O lado que escreve num canal com envelope: um `WritableStream` de bytes,
 * em pedaços de no máximo [`MAX_DADOS`]. `fila` é a fila do volume (os "u" e
 * "b"), ou `null` para o controle, que sai na hora.
 */
function escritaDoCanal(canal, { fila = null, fecharNoFim = true, esperarAEntrega = false, credito = null, ouvir = null } = {}) {
  let acabou = false;
  // A entrega de um «u» desta página: o 0x01 que quem lê devolve ao receber o
  // fim (`enlace-api.md`). Um corte no lugar dele é a recusa de quem lê. É
  // também por este ouvido que chega o crédito (0x04) de um «u»: ninguém mais
  // lê um canal que a página só escreve.
  let resolverAEntrega = null;
  if (esperarAEntrega) {
    (ouvir ?? ((fazer) => canal.addEventListener("message", fazer)))(({ data }) => {
      const lido = lerEnvelope(data);
      if (!lido) return;
      if (lido.tag === ENVELOPE.credito) {
        credito?.subir(lido.limite);
        return;
      }
      if (lido.tag === ENVELOPE.corte) credito?.fechar({ corte: lido.codigo });
      if (!resolverAEntrega) return;
      if (lido.tag === ENVELOPE.fim) resolverAEntrega(true);
      else if (lido.tag === ENVELOPE.corte) resolverAEntrega({ corte: lido.codigo });
    });
  }
  async function mandar(bytes) {
    // **O crédito do anfitrião primeiro** (0x04), e só então a vez na fila do
    // volume: quem espera crédito não segura reserva nenhuma, e os outros
    // canais andam. Só os corpos de dados contam.
    const corpo = bytes[0] === ENVELOPE.dados ? bytes.byteLength - 1 : 0;
    if (credito && corpo > 0) await credito.esperar(corpo);
    // A reserva dos bytes deste pedaço, devolvida mande ou não (`finally`):
    // no envio o crédito passa ao `bufferedAmount` do canal; no erro, volta.
    const reserva = fila ? await fila.vez(bytes.byteLength, canal) : null;
    try {
      if (canal.readyState !== "open") throw "caiu";
      canal.send(bytes);
      if (credito && corpo > 0) credito.contar(corpo);
    } finally {
      reserva?.devolver();
    }
  }
  return new WritableStream({
    async write(pedaco) {
      if (acabou) throw "fechado";
      const bytes = pedaco instanceof Uint8Array ? pedaco : new Uint8Array(pedaco);
      for (let inicio = 0; inicio < bytes.length; inicio += MAX_DADOS) {
        await mandar(envelopar(ENVELOPE.dados, bytes.subarray(inicio, inicio + MAX_DADOS)));
      }
    },
    async close() {
      acabou = true;
      const entregue = esperarAEntrega
        ? new Promise((resolver) => {
            resolverAEntrega = resolver;
            canal.addEventListener("close", () => resolver("caiu"), { once: true });
          })
        : null;
      await mandar(envelopar(ENVELOPE.fim));
      if (entregue) {
        // O `close` resolve quando quem lê confirmou tudo, como o
        // `Envio::entregue` do enlace; sem confirmação no prazo, é um erro.
        const resultado = await esperarNoMaximo(entregue, PRAZO_DA_ENTREGA_MS, "prazo-da-entrega");
        if (resultado !== true) throw resultado;
      }
      if (fecharNoFim) {
        await esvaziar(canal);
        canal.close();
      }
    },
    async abort(motivo) {
      acabou = true;
      const codigo = typeof motivo === "number" ? motivo : 0;
      try {
        if (canal.readyState === "open") canal.send(envelopeDoCorte(codigo));
        if (fecharNoFim) {
          await esvaziar(canal);
          canal.close();
        }
      } catch {
        // Já fechado.
      }
    },
  });
}

/** Espera um canal abrir, ou falha com o canal fechando. */
function canalAberto(canal) {
  if (canal.readyState === "open") return Promise.resolve(canal);
  return new Promise((resolver, rejeitar) => {
    canal.addEventListener("open", () => resolver(canal), { once: true });
    canal.addEventListener("close", () => rejeitar("caiu"), { once: true });
  });
}

/** Espera uma promessa até um prazo; vencido, rejeita com `motivo`. */
function esperarNoMaximo(promessa, ms, motivo) {
  let relogio;
  const prazo = new Promise((_, rejeitar) => {
    relogio = setTimeout(() => rejeitar(motivo), ms);
  });
  return Promise.race([promessa, prazo]).finally(() => clearTimeout(relogio));
}

/**
 * **O enlace de uma tentativa**, com a forma que a sessão espera.
 *
 * - `alvos`: os endereços do convite, na ordem dele (`alvo` e `alt=`), todos
 *   no mesmo agente ICE;
 * - `impressao`: o `fp=` em hex, que vira o `a=fingerprint` do anfitrião;
 * - `certificado`: o de [`prepararCertificadoDtls`];
 * - `bilhete`: `{ ponto, aviso }` do `enc=`, ou `null`. Sem ele não há STUN
 *   nem sinal, e só o caminho direto é tentado;
 * - `sinal`: `{ montar(json) → {corpo|erro}, ler(texto) → json }`, as funções
 *   do `.wasm` (`sinal_chama`, `sinal_resposta`);
 * - `urlDoSinal`: troca o endereço do sinal (a bancada usa um local);
 * - `aoMudar(etapa)`: cada passo, para a casca dizer onde está;
 * - `fetch`: o da página, trocado na bancada.
 */
function criarEnlaceRtc({
  alvos,
  impressao,
  certificado,
  bilhete = null,
  sinal = null,
  urlDoSinal = urlDoSinalDoPonto,
  aoMudar = () => {},
  prazo = PRAZO_DO_ENLACE_MS,
  limiarDoVolume = LIMIAR_DO_VOLUME,
  fetch: buscar = (...args) => globalThis.fetch(...args),
}) {
  // O parâmetro com padrão, num nome declarado: o guarda das chamadas
  // (`no_script_calls_a_function_that_no_script_declares`) lê por linha, e não
  // vê a desestruturação de várias linhas.
  const enderecoDoSinal = urlDoSinal;
  const credencial = sortearCredencialIce();
  const pc = new RTCPeerConnection({
    iceServers: servidoresIce({ ponto: bilhete?.ponto ?? null }),
    certificates: [certificado.certificado],
    bundlePolicy: "max-bundle",
  });
  const ctl = pc.createDataChannel("ctl", { negotiated: true, id: 0, priority: "high" });
  const voz = pc.createDataChannel("voz", { negotiated: true, id: 1, ordered: false, maxRetransmits: 0, priority: "high" });
  ctl.binaryType = "arraybuffer";
  voz.binaryType = "arraybuffer";

  const fila = criarAFilaDoVolume(limiarDoVolume);
  const contas = {
    canaisRecusados: 0,
    cortesPorFila: 0,
    vozDescartada: 0,
    candidatosRecusados: 0,
    // Mensagens que chegaram num canal desta página antes de quem o lê se ligar
    // a ele (guardadas e entregues em ordem, e não perdidas).
    mensagensAntesDoOuvinte: 0,
    // O crédito (0x04): os anúncios desta página, os do anfitrião, e quantas
    // vezes um pedaço esperou crédito para sair.
    creditosAnunciados: 0,
    creditosRecebidos: 0,
    esperasPorCredito: 0,
  };
  const creditoDaConexao = criarCreditoDaConexao({ contas });
  // Os créditos do anfitrião de cada canal que a página escreve: o fecho do
  // enlace solta quem espera crédito (o `pc.close()` não avisa cada canal).
  const creditosDoPar = new Set();
  const creditoDoPar = (canal) => {
    const credito = criarCreditoDoPar(canal, contas);
    creditosDoPar.add(credito);
    canal.addEventListener("close", () => creditosDoPar.delete(credito), { once: true });
    return credito;
  };
  /** Os `srflx` que o navegador contou, sem repetir, na ordem em que chegaram. */
  const refletidos = [];
  const visto = { refletido: null, sinal: null, dtls: null, candidatos: [], venceu: null, soNomes: false };
  let fechado = false;
  let requisicaoDoSinal = null;
  let descobrindoCaminho = Boolean(bilhete);
  let ultimaVoz = performance.now();

  // ------------------------------------------------------------ o fecho
  let resolverFechado;
  const closed = new Promise((resolver) => {
    resolverFechado = resolver;
  });
  function encerrar(motivo) {
    if (fechado) return;
    fechado = true;
    requisicaoDoSinal?.abort();
    clearInterval(keepalive);
    fila.fechar("caiu");
    for (const credito of creditosDoPar) credito.fechar("caiu");
    controle.caiu();
    for (const um of leiturasAbertas) um.caiu();
    try {
      pc.close();
    } catch {
      // Já fechado.
    }
    resolverFechado(motivo);
  }
  // **O primeiro estado terminal decide** (spike C, 24/10/2026). O navegador
  // não diz por que o DTLS falhou: não há `onerror` nem `errorDetail` no
  // Chromium 149 nem no WebKit 26.5. O que se lê é o ICE `connected` com o DTLS
  // (ou a conexão) em `failed`, 40–60 ms depois. Isso diz que a conexão segura
  // não se verificou, e não por quê: a impressão diferente é um dos motivos, e
  // não está provada (`FALHA_DO_ENLACE.verificacaoFalhou`). O `pc` fecha na
  // hora: o ICE cai depois (a lápide do anfitrião) e não pode trocar essa falha
  // por «caminho não abriu», e os canais ficariam em `connecting` para sempre.
  const iceAberto = () => pc.iceConnectionState === "connected" || pc.iceConnectionState === "completed";
  function olharOEstado() {
    if (fechado) return;
    const dtls = pc.sctp?.transport?.state;
    if ((pc.connectionState === "failed" || dtls === "failed") && iceAberto() && !visto.dtls) visto.dtls = "failed";
    const falhaFinal = !descobrindoCaminho || iceAberto();
    if ((pc.connectionState === "failed" && falhaFinal) || pc.connectionState === "closed" || dtls === "failed") {
      encerrar({ caiu: pc.connectionState, dtls: dtls ?? null });
    }
  }
  pc.addEventListener("connectionstatechange", olharOEstado);
  pc.addEventListener("iceconnectionstatechange", () => {
    if (pc.iceConnectionState === "failed" && !descobrindoCaminho) encerrar({ caiu: "ice" });
  });
  ctl.addEventListener("close", () => encerrar({ caiu: "ctl" }));

  // ------------------------------------------------------- o controle
  // A primeira mensagem do controle é a apresentação (0x03) do anfitrião; o
  // resto é o fluxo de controle de hoje, no envelope.
  let resolverApresentacao;
  let rejeitarApresentacao;
  const apresentada = new Promise((resolver, rejeitar) => {
    resolverApresentacao = resolver;
    rejeitarApresentacao = rejeitar;
  });
  let apresentacao = null;
  // O controle lê com janela própria (fora da soma da conexão) e anuncia ao
  // abrir, como quem liga no enlace nativo; o crédito do anfitrião no controle
  // é de quem escreve nele (`creditoDoControle`).
  const creditoDoControle = creditoDoPar(ctl);
  const janelaDoControle = creditoDaConexao.abrir(ctl, { fixo: true });
  const controle = leituraDoCanal(ctl, { janela: janelaDoControle, aoCredito: (limite) => creditoDoControle.subir(limite), controle: true });
  canalAberto(ctl).then(() => janelaDoControle.anunciar(), () => {});
  ctl.addEventListener("message", ({ data }) => {
    if (apresentacao === null) {
      const lido = lerEnvelope(data);
      if (!lido || lido.tag !== ENVELOPE.apresentacao) {
        rejeitarApresentacao(FALHA_DO_ENLACE.protocolo);
        try {
          ctl.send(envelopeDoCorte(CORTE.violacao));
        } catch {
          // Já fechado.
        }
        encerrar({ protocolo: "sem-apresentacao" });
        return;
      }
      apresentacao = lido.corpo.slice();
      resolverApresentacao(apresentacao);
      return;
    }
    const lido = lerEnvelope(data);
    // Um corte no controle é o fecho da conexão, com o código do anfitrião.
    if (lido && lido.tag === ENVELOPE.corte) {
      controle.chegou(data);
      encerrar({ fechada: lido.codigo });
      return;
    }
    controle.chegou(data);
  });
  let controleEntregue = false;

  // ---------------------------------------------------------- a voz
  let controladorDaVoz = null;
  const leituraDaVoz = new ReadableStream({
    start(c) {
      controladorDaVoz = c;
    },
  });
  voz.addEventListener("message", ({ data }) => {
    const bytes = new Uint8Array(data);
    // O keepalive (um 0x00) nunca é voz: a voz começa pelo byte de versão.
    if (bytes.length === 1 && bytes[0] === 0) return;
    if (bytes.length > MAX_VOZ) {
      contas.vozDescartada += 1;
      return;
    }
    try {
      controladorDaVoz.enqueue(bytes);
    } catch {
      // A leitura foi cancelada.
    }
  });
  const escritaDaVoz = new WritableStream({
    write(bytes) {
      if (fechado || voz.readyState !== "open") return;
      const dados = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
      if (dados.length > MAX_VOZ) {
        contas.vozDescartada += 1;
        return;
      }
      voz.send(dados);
      ultimaVoz = performance.now();
    },
  });
  const keepalive = setInterval(() => {
    if (fechado || voz.readyState !== "open") return;
    if (performance.now() - ultimaVoz >= KEEPALIVE_MS) {
      voz.send(new Uint8Array([0]));
      ultimaVoz = performance.now();
    }
  }, KEEPALIVE_MS);

  // ------------------------------------------- os fluxos do anfitrião
  const leiturasAbertas = new Set();
  let controladorDosFluxos = null;
  const incomingUnidirectionalStreams = new ReadableStream({
    start(c) {
      controladorDosFluxos = c;
    },
  });
  pc.addEventListener("datachannel", ({ channel }) => {
    channel.binaryType = "arraybuffer";
    if (channel.protocol !== PROTOCOLO_DO_CANAL || channel.label !== "u") {
      // Um canal que não é do contrato é fechado e contado (`enlace-api.md`).
      contas.canaisRecusados += 1;
      channel.addEventListener("open", () => {
        try {
          channel.send(envelopeDoCorte(CORTE.desconhecido));
        } catch {
          // Já fechado.
        }
        channel.close();
      });
      return;
    }
    const janela = creditoDaConexao.abrir(channel);
    const lado = leituraDoCanal(channel, { aoCortarPorFila: () => (contas.cortesPorFila += 1), devolverOFim: true, janela });
    leiturasAbertas.add(lado);
    channel.addEventListener("message", ({ data }) => lado.chegou(data));
    canalAberto(channel).then(() => janela.anunciar(), () => {});
    channel.addEventListener("close", () => {
      lado.caiu();
      leiturasAbertas.delete(lado);
    });
    try {
      controladorDosFluxos.enqueue(lado.leitura);
    } catch {
      // Ninguém lê mais os fluxos.
    }
  });

  /**
   * Um canal extra desta página, já aberto, e o `ouvir` dele. **O ouvido nasce
   * com o canal**: o anfitrião anuncia o crédito (0x04) assim que vê o canal
   * abrir, e uma mensagem que chegasse antes de quem escreve se ligar ao canal
   * se perderia (e quem escreve esperaria para sempre). Defensivo: em 24/10 a
   * conta `mensagensAntesDoOuvinte` ficou em 0 em todas as rodadas medidas, e a
   * espera sem fim de um «u» vista duas vezes NÃO foi explicada por isto. O
   * que chega antes fica guardado e é entregue, em ordem, a quem ouvir.
   */
  async function abrirCanal(rotulo) {
    if (fechado) throw "caiu";
    const canal = pc.createDataChannel(rotulo, { protocol: PROTOCOLO_DO_CANAL, priority: "very-low" });
    canal.binaryType = "arraybuffer";
    const antes = [];
    let ouvinte = null;
    canal.addEventListener("message", (evento) => {
      if (ouvinte) ouvinte(evento);
      else {
        contas.mensagensAntesDoOuvinte += 1;
        antes.push(evento);
      }
    });
    fila.juntar(canal);
    canal.addEventListener("close", () => fila.largar(canal));
    await canalAberto(canal);
    const ouvir = (fazer) => {
      ouvinte = fazer;
      for (const evento of antes.splice(0)) fazer(evento);
    };
    return { canal, ouvir };
  }

  // -------------------------------------------------------- o discar
  async function discar() {
    const oferta = await pc.createOffer();
    const sdp = ofertaComCredencial(oferta.sdp, credencial);
    const mid = (sdp.match(/a=mid:(\S+)/) || [])[1] || "0";
    pc.addEventListener("icecandidate", ({ candidate }) => {
      if (!candidate || !candidate.candidate) return;
      // Sem endereço (o navegador o esconde) não há o que mandar ao sinal.
      if (!candidate.address || !candidate.port) return;
      const contado = escreverEndereco(candidate.address, candidate.port);
      if (candidate.type === "srflx") {
        if (!refletidos.includes(contado)) refletidos.push(contado);
        // Quem lê o `visto` continua vendo o primeiro.
        if (!visto.refletido) visto.refletido = contado;
      }
    });
    await pc.setLocalDescription({ type: "offer", sdp });
    // Os endereços IP do convite vão na própria descrição, como no spike C; um
    // nome (DDNS) vai depois, pelo `addIceCandidate`, para um que o navegador
    // recuse não derrubar a descrição. Sem ponto, a lista acabou ali
    // (`a=end-of-candidates`); com ponto, o `AQUI` ainda pode trazer outros.
    const separados = alvos.map((alvo) => ({ alvo, separado: separarAlvo(alvo, 8383) }));
    const ips = separados.filter(({ separado }) => separado && separado.familia !== 0);
    const nomes = separados.filter(({ separado }) => separado && separado.familia === 0).map(({ alvo }) => alvo);
    contas.candidatosRecusados += separados.filter(({ separado }) => !separado).length;
    const linhas = ips.map(({ separado }, indice) => `a=${linhaDoCandidato(indice, separado)}`);
    visto.candidatos.push(...ips.map(({ alvo }) => alvo));
    visto.soNomes = ips.length === 0 && nomes.length > 0;
    const remota = descricaoDoAnfitriao({ mid, credencial, impressao, candidatos: linhas, acabou: !bilhete && nomes.length === 0 });
    await pc.setRemoteDescription({ type: "answer", sdp: remota });
    pc.sctp?.transport?.addEventListener("statechange", olharOEstado);
    await somarCandidatos(mid, nomes);
    aoMudar({ Discando: { candidatos: visto.candidatos.length } });
    return mid;
  }

  /** Põe candidatos do anfitrião no agente ICE, um por um. */
  async function somarCandidatos(mid, enderecos) {
    for (const endereco of enderecos) {
      const separado = separarAlvo(endereco, 8383);
      if (!separado) {
        contas.candidatosRecusados += 1;
        continue;
      }
      const linha = linhaDoCandidato(visto.candidatos.length, separado);
      try {
        await pc.addIceCandidate({ candidate: linha, sdpMid: mid, sdpMLineIndex: 0 });
        visto.candidatos.push(endereco);
      } catch (falha) {
        contas.candidatosRecusados += 1;
        console.warn(`enlace: o navegador recusou o candidato ${endereco}:`, falha);
      }
    }
  }

  const ehIpv6 = (texto) => texto.startsWith("[");
  const temAsDuasFamilias = () => refletidos.some((c) => !ehIpv6(c)) && refletidos.some(ehIpv6);

  /**
   * Os `srflx` que vão na chamada: no máximo [`LIMITE_DE_SRFLX`], a diversidade
   * primeiro (um v4 e um v6, os dois; dois da mesma família só se não houver da
   * outra), na ordem em que chegaram. Só `srflx`: nenhum `host`, nenhum mDNS.
   */
  function osSrflxQueVao() {
    const escolhidos = new Set([refletidos.find((c) => !ehIpv6(c)), refletidos.find(ehIpv6)].filter(Boolean));
    for (const candidato of refletidos) {
      if (escolhidos.size >= LIMITE_DE_SRFLX) break;
      escolhidos.add(candidato);
    }
    return refletidos.filter((c) => escolhidos.has(c));
  }

  /**
   * Espera os refletidos pelo STUN do ponto: sai no que vier antes entre já haver
   * um v4 e um v6, a coleta completa, e o teto de [`ESPERA_DO_REFLETIDO_MS`]
   * Esta espera só agrupa o primeiro anúncio. Um `srflx` tardio ainda pode
   * atualizar o mesmo par durante a janela limitada de sinalização.
   */
  async function esperarORefletido() {
    const ate = performance.now() + ESPERA_DO_REFLETIDO_MS;
    while (!temAsDuasFamilias() && pc.iceGatheringState !== "complete" && performance.now() < ate && !fechado) {
      await new Promise((resolver) => setTimeout(resolver, 50));
    }
  }

  /** **A chamada pelo ponto** (D7): o `POST /sinal/1`, e os candidatos do `AQUI`. */
  async function anunciarPeloPonto(mid, url, candidatos) {
    if (!url || !sinal) {
      visto.sinal = { semSinal: true };
      return;
    }
    const montado = sinal.montar({ impressao, credencial, candidatos });
    if (!montado.corpo) {
      visto.sinal = { erro: montado.erro };
      return;
    }
    aoMudar({ ChamandoPeloPonto: { ponto: bilhete.ponto } });
    // Se o prazo vencer com a chamada no ar, ficou assim: o ponto não respondeu à página.
    visto.sinal = { pendente: true };
    let texto;
    const abortador = new AbortController();
    requisicaoDoSinal = abortador;
    try {
      texto = await esperarNoMaximo((async () => {
        const resposta = await buscar(url, {
          method: "POST",
          body: montado.corpo,
          headers: { "Content-Type": "text/plain" },
          mode: "cors",
          credentials: "omit",
          cache: "no-store",
          referrerPolicy: "no-referrer",
          signal: abortador.signal,
        });
        if (!resposta.ok) throw `http ${resposta.status}`;
        return resposta.text();
      })(),
        PRAZO_DO_SINAL_MS,
        "prazo",
      );
    } catch (falha) {
      if (!fechado) visto.sinal = { mudo: String(falha) };
      return;
    } finally {
      abortador.abort();
      if (requisicaoDoSinal === abortador) requisicaoDoSinal = null;
    }
    if (fechado) return;
    const lido = sinal.ler(texto);
    visto.sinal = lido;
    if (lido.aqui) await somarCandidatos(mid, [lido.aqui.endereco, ...(lido.aqui.dicas ?? [])]);
  }

  async function chamarPeloPonto(mid) {
    try {
      await atualizarCaminhoPeloPonto(mid);
    } finally {
      descobrindoCaminho = false;
      olharOEstado();
      if (!fechado && pc.iceConnectionState === "failed") encerrar({ caiu: "ice" });
    }
  }

  async function atualizarCaminhoPeloPonto(mid) {
    const url = enderecoDoSinal(bilhete.ponto);
    await esperarORefletido();
    if (fechado) return;
    const ate = performance.now() + JANELA_DE_ATUALIZACAO_MS;
    const anunciados = new Set();
    let chamadas = 0;
    while (!fechado && !iceAberto() && ctl.readyState !== "open" && chamadas < LIMITE_DE_CHAMADAS_AO_PONTO) {
      if (chamadas > 0 && performance.now() >= ate) return;
      const atuais = chamadas === 0 ? osSrflxQueVao() : refletidos;
      const novos = atuais.filter((c) => !anunciados.has(c)).slice(0, LIMITE_DE_ANUNCIADOS - anunciados.size);
      if (chamadas === 0 || novos.length) {
        // Novos primeiro: não gastar os dois lugares repetindo endereços que
        // o anfitrião já conhece. A credencial e o certificado não mudam.
        const candidatos = [...novos, ...atuais.filter((c) => anunciados.has(c))].slice(0, LIMITE_DE_SRFLX);
        for (const c of candidatos) anunciados.add(c);
        chamadas += 1;
        visto.chamadasAoPonto = chamadas;
        visto.candidatosAnunciados = anunciados.size;
        await anunciarPeloPonto(mid, url, candidatos);
        if (visto.sinal?.ninguem || visto.sinal?.recusa || visto.sinal?.erro || visto.sinal?.semSinal) return;
      }
      if (fechado || performance.now() >= ate || anunciados.size >= LIMITE_DE_ANUNCIADOS) return;
      // Depois de complete ainda podemos ter recebido um candidato durante
      // o POST; só encerrar a coleta após conferir se falta anunciá-lo.
      if (pc.iceGatheringState === "complete" && !refletidos.some((c) => !anunciados.has(c))) return;
      await new Promise((resolver) => setTimeout(resolver, 50));
    }
  }

  /** O que deu errado, com as medidas desta tentativa. */
  function classificar(motivo) {
    if (visto.dtls) return FALHA_DO_ENLACE.verificacaoFalhou;
    if (motivo === FALHA_DO_ENLACE.protocolo) return FALHA_DO_ENLACE.protocolo;
    if (pc.iceConnectionState === "connected" || pc.iceConnectionState === "completed") {
      // O ICE abriu e o DTLS não: a verificação falhou, sem motivo legível.
      return pc.connectionState === "failed" ? FALHA_DO_ENLACE.verificacaoFalhou : FALHA_DO_ENLACE.prazo;
    }
    // Sem ponto: o caminho direto era o único. Se o link só tinha nomes, a frase
    // diz isso (o nome pode não ter resolvido, ou o servidor não respondeu nele).
    if (!bilhete) return visto.soNomes ? FALHA_DO_ENLACE.nomeSemResposta : FALHA_DO_ENLACE.semCaminho;
    const s = visto.sinal;
    if (s?.ninguem) return FALHA_DO_ENLACE.ninguemNoPonto;
    if (s?.calado) return FALHA_DO_ENLACE.anfitriaoCalado;
    if (s?.recusa) return FALHA_DO_ENLACE.pontoRecusou;
    if (!visto.refletido) return FALHA_DO_ENLACE.semRefletido;
    if (s?.mudo || s?.semSinal || s?.erro || s?.pendente) return FALHA_DO_ENLACE.sinalMudo;
    return FALHA_DO_ENLACE.caminhoNaoAbriu;
  }

  const ready = (async () => {
    // Os canais de um `pc` que falhou ficam em `connecting` para sempre: o
    // fecho (e o prazo) são daqui, e não deles.
    const caiuAntes = closed.then((motivo) => Promise.reject(motivo?.falha ?? "fechado"));
    caiuAntes.catch(() => {});
    const aberto = Promise.race([Promise.all([canalAberto(ctl), canalAberto(voz), apresentada]), caiuAntes]);
    aberto.catch(() => {});
    let falha = null;
    const inicio = performance.now();
    try {
      // **Um prazo só, para a tentativa inteira.** Só o último `await` tinha
      // prazo: o `discar()` (`setLocalDescription`, `setRemoteDescription`,
      // `addIceCandidate`) e a chamada pelo ponto (o corpo da resposta, os
      // candidatos do `AQUI`) esperavam o navegador sem limite, e uma espera que
      // não volta (o Safari faz isso) deixava o `ready` pendente para sempre: a
      // tela ficava em «conectando» sem erro nenhum. Agora o `prazo` cobre tudo, e
      // vencido ele cai no `catch` abaixo, que classifica, fecha o
      // `RTCPeerConnection` e rejeita com um dos motivos de sempre.
      const tentativa = (async () => {
        const mid = await discar();
        if (bilhete) {
          const direto = await Promise.race([
            aberto.then(
              () => true,
              () => true,
            ),
            new Promise((resolver) => setTimeout(() => resolver(false), ESPERA_DO_DIRETO_MS)),
          ]);
          if (!direto && !fechado) await chamarPeloPonto(mid);
        }
        await aberto;
      })();
      // Depois do prazo ela pode rejeitar sobre um `pc` já fechado: ninguém a espera.
      tentativa.catch(() => {});
      await esperarNoMaximo(tentativa, prazo, FALHA_DO_ENLACE.prazo);
      descobrindoCaminho = false;
    } catch (motivo) {
      falha = motivo;
    }
    if (falha !== null || fechado) {
      const classificada = classificar(falha);
      const detalhe = {
        ice: pc.iceConnectionState,
        conexao: pc.connectionState,
        refletido: Boolean(visto.refletido),
        sinal: visto.sinal,
        dtls: visto.dtls,
        candidatos: visto.candidatos.length,
      };
      encerrar({ falha: classificada });
      throw { FalhaDoEnlace: { motivo: classificada, detalhe } };
    }
    visto.venceu = await candidatoVencedor();
    aoMudar({ Aberto: { por: visto.venceu } });
  })();

  /** O par de candidatos que o ICE escolheu: `endereço:porta` do anfitrião. */
  async function candidatoVencedor() {
    try {
      // Sem resposta no prazo, rejeita e cai no `catch`: `null`, nunca um valor inventado.
      const estatisticas = await esperarNoMaximo(pc.getStats(), ESPERA_DO_PAR_VENCEDOR_MS, "sem-telemetria");
      let par = null;
      estatisticas.forEach((uma) => {
        if (uma.type === "transport" && uma.selectedCandidatePairId) par = estatisticas.get(uma.selectedCandidatePairId);
      });
      if (!par) estatisticas.forEach((uma) => {
        if (uma.type === "candidate-pair" && uma.nominated && uma.state === "succeeded") par = uma;
      });
      const remoto = par ? estatisticas.get(par.remoteCandidateId) : null;
      if (!remoto) return null;
      // O WebKit 26.5 dá o candidato remoto sem endereço (medido: «:63118»):
      // sem ele, `null`, e a volta usa os alvos do convite.
      const maquina = String(remoto.address || remoto.ip || "");
      if (!maquina || !remoto.port) return null;
      return escreverEndereco(maquina, remoto.port);
    } catch {
      return null;
    }
  }

  return {
    ready,
    closed,
    credencial,
    impressaoDtls: certificado.impressao,
    contas,
    /** A apresentação (0x03) do anfitrião, depois do `ready`. */
    get apresentacao() {
      return apresentacao;
    },
    /** O endereço do anfitrião pelo qual o ICE abriu, depois do `ready`. */
    get atendeuPor() {
      return visto.venceu;
    },
    /** O que esta tentativa viu (o sinal, o refletido), para o registro. */
    get visto() {
      return { ...visto };
    },
    /** O fluxo de controle: o canal 0, entregue uma vez. */
    controle() {
      if (controleEntregue) throw "o controle já foi entregue";
      controleEntregue = true;
      return { readable: controle.leitura, writable: escritaDoCanal(ctl, { fecharNoFim: false, credito: creditoDoControle }) };
    },
    async createUnidirectionalStream() {
      const { canal, ouvir } = await abrirCanal("u");
      return escritaDoCanal(canal, { fila, esperarAEntrega: true, credito: creditoDoPar(canal), ouvir });
    },
    async createBidirectionalStream() {
      const { canal, ouvir } = await abrirCanal("b");
      const credito = creditoDoPar(canal);
      const janela = creditoDaConexao.abrir(canal);
      const lado = leituraDoCanal(canal, {
        aoCortarPorFila: () => (contas.cortesPorFila += 1),
        janela,
        aoCredito: (limite) => credito.subir(limite),
      });
      leiturasAbertas.add(lado);
      ouvir(({ data }) => lado.chegou(data));
      canal.addEventListener("close", () => {
        lado.caiu();
        leiturasAbertas.delete(lado);
      });
      // Quem abre um "b" também o lê: anuncia a janela já aberto.
      janela.anunciar();
      // No "b", cada lado manda o seu fim, e o canal fecha quando os dois
      // passaram: quem escreve não fecha sozinho.
      let fimDele = false;
      let fimMeu = false;
      const fecharSeAcabou = () => {
        if (fimDele && fimMeu) esvaziar(canal).then(() => canal.close());
      };
      const leituraComFim = lado.leitura.pipeThrough(
        new TransformStream({
          flush() {
            fimDele = true;
            fecharSeAcabou();
          },
        }),
      );
      const escrita = escritaDoCanal(canal, { fila, fecharNoFim: false, credito });
      const escritor = escrita.getWriter();
      const escritaComFim = new WritableStream({
        write: (pedaco) => escritor.write(pedaco),
        async close() {
          await escritor.close();
          fimMeu = true;
          fecharSeAcabou();
        },
        abort: (motivo) => escritor.abort(motivo),
      });
      return { readable: leituraComFim, writable: escritaComFim };
    },
    incomingUnidirectionalStreams,
    canalDaVoz: { readable: leituraDaVoz, writable: escritaDaVoz },
    /**
     * As contas do navegador para cada canal (`getStats`, tipo `data-channel`):
     * `{ [label]: { mensagensEnviadas, mensagensRecebidas, bytesEnviados, bytesRecebidos } }`.
     * É o que diz quantos datagramas de voz saíram e chegaram, sem eco.
     */
    async contasDosCanais() {
      const estatisticas = await pc.getStats();
      const contas = {};
      estatisticas.forEach((uma) => {
        if (uma.type !== "data-channel") return;
        const nome = uma.label ?? String(uma.dataChannelIdentifier);
        const antes = contas[nome] ?? { mensagensEnviadas: 0, mensagensRecebidas: 0, bytesEnviados: 0, bytesRecebidos: 0 };
        contas[nome] = {
          mensagensEnviadas: antes.mensagensEnviadas + (uma.messagesSent ?? 0),
          mensagensRecebidas: antes.mensagensRecebidas + (uma.messagesReceived ?? 0),
          bytesEnviados: antes.bytesEnviados + (uma.bytesSent ?? 0),
          bytesRecebidos: antes.bytesRecebidos + (uma.bytesReceived ?? 0),
        };
      });
      return contas;
    },
    /** A medida do ICE, na forma que a sessão lia da WebTransport (ms). */
    async getStats() {
      const estatisticas = await pc.getStats();
      let rtt = null;
      estatisticas.forEach((uma) => {
        if (uma.type === "candidate-pair" && uma.nominated && typeof uma.currentRoundTripTime === "number") {
          rtt = uma.currentRoundTripTime * 1000;
        }
      });
      return { smoothedRtt: rtt, minRtt: rtt, emEspera: fila.soma(), creditoEmAberto: creditoDaConexao.emAberto() };
    },
    close(codigo = 0) {
      if (fechado) return;
      try {
        if (ctl.readyState === "open") ctl.send(envelopeDoCorte(codigo));
      } catch {
        // Já fechado.
      }
      encerrar({ local: codigo });
    },
  };
}
