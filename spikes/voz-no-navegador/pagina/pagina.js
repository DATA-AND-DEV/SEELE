// O lado do navegador do spike. Sem framework e sem npm, como a casca
// (ADR 0019). Tudo o que ele mede volta ao servidor: pelo WebTransport a cada
// cinco segundos, e por HTTPS quando o WebTransport nem abre ou a página some.
// Uma falha que só aparece na tela de um celular que não está na nossa mão é
// uma falha que ninguém vê.

"use strict";

const QUADRO = 960;
const ENCODER = {
  codec: "opus",
  sampleRate: 48000,
  numberOfChannels: 1,
  bitrate: 32000,
  opus: { frameDuration: 20000 },
};
const DECODER = { codec: "opus", sampleRate: 48000, numberOfChannels: 1 };

const inicio = performance.now();
const seg = () => Math.round(performance.now() - inicio) / 1000;
const $ = (id) => document.getElementById(id);

const estado = {
  cap: null,
  conexao: { ok: false, ms: null, erro: null, maxDatagrama: null, escrita: null, fechou: null },
  rtt: { amostras: [], enviados: 0, voltaram: 0 },
  voz: {
    enviados: 0, recebidos: 0, perdidos: 0, fora_de_ordem: 0,
    eco: [], ecoTotal: 0, codificados: 0, erros: [],
    faltas: 0, descartes: 0, bufferMs: {},
  },
  audio: null,
  oculto: {
    vezes: 0, msTotal: 0, desde: null, enviadosOculto: 0, recebidosOculto: 0,
    // O que o áudio fez com a página oculta, contado pelos worklets: quadros
    // de 20 ms que saíram do microfone, e blocos de 128 amostras tocados com som.
    capturadosOculto: 0, blocosOculto: 0, comSomOculto: 0,
  },
  eventos: [],
};

function evento(texto, dados) {
  const linha = { t: seg(), texto, ...(dados ? { dados } : {}) };
  estado.eventos.push(linha);
  if (estado.eventos.length > 80) estado.eventos.shift();
  const li = document.createElement("li");
  li.textContent = `${linha.t.toFixed(1)} s · ${texto}${dados ? " · " + JSON.stringify(dados) : ""}`;
  $("registro").prepend(li);
}

function percentil(lista, p) {
  if (!lista.length) return null;
  const ordenada = [...lista].sort((a, b) => a - b);
  return Math.round(ordenada[Math.min(ordenada.length - 1, Math.floor(p * ordenada.length))] * 10) / 10;
}

// ---- capacidades ----------------------------------------------------------

async function medirCapacidades() {
  const c = {
    ua: navigator.userAgent,
    seguro: window.isSecureContext,
    webtransport: "WebTransport" in window,
    audioEncoder: "AudioEncoder" in window,
    audioDecoder: "AudioDecoder" in window,
    audioWorklet: "AudioWorkletNode" in window,
    getUserMedia: !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia),
    wakeLock: "wakeLock" in navigator,
    mediaSession: "mediaSession" in navigator,
    audioSession: "audioSession" in navigator,
    // Aberta pelo ícone da Tela de Início (web app) ou numa aba do Safari: o
    // relatório não distinguia, e a tela bloqueada pode se comportar diferente.
    webApp: window.matchMedia("(display-mode: standalone)").matches || navigator.standalone === true,
    busca: location.search || "(nenhuma)",
  };
  if (c.audioEncoder) {
    try {
      c.opusEncoder = (await AudioEncoder.isConfigSupported(ENCODER)).supported;
    } catch (e) {
      c.opusEncoder = "erro: " + e.message;
    }
  }
  if (c.audioDecoder) {
    try {
      c.opusDecoder = (await AudioDecoder.isConfigSupported(DECODER)).supported;
    } catch (e) {
      c.opusDecoder = "erro: " + e.message;
    }
  }
  return c;
}

function pintarCapacidades(c) {
  const nomes = {
    seguro: "contexto seguro", webtransport: "WebTransport", audioEncoder: "AudioEncoder",
    opusEncoder: "Opus no encoder", audioDecoder: "AudioDecoder", opusDecoder: "Opus no decoder",
    audioWorklet: "AudioWorklet", getUserMedia: "microfone", wakeLock: "Wake Lock",
    mediaSession: "Media Session", audioSession: "Audio Session",
  };
  const ul = $("capacidades");
  ul.textContent = "";
  for (const [chave, nome] of Object.entries(nomes)) {
    if (!(chave in c)) continue;
    const li = document.createElement("li");
    const v = c[chave];
    // Nunca só a cor: a palavra diz o resultado (specs/06-clientes-gui.md).
    li.dataset.ok = v === true ? "sim" : "nao";
    li.textContent = `${nome}: ${v === true ? "sim" : v === false ? "NÃO" : v}`;
    ul.append(li);
  }
  $("ua").textContent = c.ua;
}

// ---- relatório -----------------------------------------------------------

function relatorio(motivo) {
  const v = estado.voz;
  const r = estado.rtt;
  const ua = estado.cap ? estado.cap.ua : navigator.userAgent;
  const curto = (ua.match(/(iPhone|iPad|Android [\d.]+|Macintosh|Windows|Linux)/) || ["?"])[0] +
    " " + ((ua.match(/(CriOS|FxiOS|EdgA|Chrome|Firefox|Version)\/[\d.]+/) || [""])[0]);
  const oculto = estado.oculto;
  const msOculto = oculto.msTotal + (oculto.desde ? performance.now() - oculto.desde : 0);
  const resumo = [
    curto,
    motivo,
    estado.conexao.ok ? `wt ok em ${Math.round(estado.conexao.ms)} ms` : `wt: ${estado.conexao.erro || "não tentou"}`,
    `rtt p50 ${percentil(r.amostras, 0.5)} p95 ${percentil(r.amostras, 0.95)} ms (${r.voltaram}/${r.enviados})`,
    `voz ↑${v.enviados} ↓${v.recebidos} perdidos ${v.perdidos}`,
    `eco p50 ${percentil(v.eco, 0.5)} ms`,
    `faltas ${v.faltas}`,
    `oculto ${oculto.vezes}× ${Math.round(msOculto / 1000)} s (↑${oculto.enviadosOculto} ↓${oculto.recebidosOculto} · mic ${oculto.capturadosOculto} quadros · tocou ${Math.round(oculto.comSomOculto * 128 / 48)} ms)`,
  ].join(" · ");
  return {
    resumo,
    motivo,
    t: seg(),
    cap: estado.cap,
    conexao: estado.conexao,
    rtt: { n: r.amostras.length, p50: percentil(r.amostras, 0.5), p95: percentil(r.amostras, 0.95), enviados: r.enviados, voltaram: r.voltaram },
    voz: {
      enviados: v.enviados, recebidos: v.recebidos, perdidos: v.perdidos, fora_de_ordem: v.fora_de_ordem,
      codificados: v.codificados, ecoN: v.ecoTotal, ecoP50: percentil(v.eco, 0.5), ecoP95: percentil(v.eco, 0.95),
      faltas: v.faltas, descartes: v.descartes, bufferMs: v.bufferMs, erros: v.erros.slice(-10),
      blocos: v.blocos ?? 0, comSom: v.comSom ?? 0,
    },
    audio: estado.audio && {
      ...estado.audio,
      estado: ctx ? ctx.state : null,
      microfone: faixa ? { readyState: faixa.readyState, muted: faixa.muted, enabled: faixa.enabled } : null,
    },
    oculto: {
      vezes: oculto.vezes, msTotal: Math.round(msOculto), enviadosOculto: oculto.enviadosOculto, recebidosOculto: oculto.recebidosOculto,
      capturadosOculto: oculto.capturadosOculto, blocosOculto: oculto.blocosOculto, comSomOculto: oculto.comSomOculto,
    },
    eventos: estado.eventos.slice(-40),
  };
}

let fluxoRelatorio = null;
const codificadorTexto = new TextEncoder();

async function mandarRelatorio(motivo) {
  const r = relatorio(motivo);
  pintarMedidas(r);
  if (fluxoRelatorio) {
    try {
      await fluxoRelatorio.write(codificadorTexto.encode(JSON.stringify(r) + "\n"));
      return;
    } catch (e) {
      evento("relatório pelo WebTransport falhou", { erro: e.message });
      fluxoRelatorio = null;
    }
  }
  mandarPorHttp(r);
}

function mandarPorHttp(r) {
  const corpo = JSON.stringify(r);
  if (navigator.sendBeacon && navigator.sendBeacon("/relatorio", new Blob([corpo], { type: "application/json" }))) return;
  fetch("/relatorio", { method: "POST", body: corpo, keepalive: true }).catch(() => {});
}

function pintarMedidas(r) {
  $("medidas").textContent = r.resumo.split(" · ").slice(2).join("\n");
}

// ---- WebTransport --------------------------------------------------------

let wt = null;
let escritor = null;

async function conectar() {
  $("conectar").disabled = true;
  const url = `https://${location.hostname}:${window.SPIKE.porta}/sala`;
  evento("conectando", { url });
  const t0 = performance.now();
  try {
    wt = new WebTransport(url, {
      serverCertificateHashes: [{ algorithm: "sha-256", value: new Uint8Array(window.SPIKE.hash) }],
    });
    await wt.ready;
  } catch (e) {
    estado.conexao.erro = `${e.name}: ${e.message}`;
    evento("WebTransport não abriu", { erro: estado.conexao.erro });
    mandarPorHttp(relatorio("falha ao conectar"));
    $("conectar").disabled = false;
    return;
  }
  estado.conexao.ok = true;
  estado.conexao.ms = performance.now() - t0;
  estado.conexao.erro = null;
  estado.conexao.maxDatagrama = wt.datagrams.maxDatagramSize ?? null;
  // A especificação trocou `datagrams.writable` por `createWritable()`. Qual
  // dos dois o aparelho tem é dado, e vai no relatório.
  if (typeof wt.datagrams.createWritable === "function") {
    escritor = wt.datagrams.createWritable().getWriter();
    estado.conexao.escrita = "createWritable";
  } else {
    escritor = wt.datagrams.writable.getWriter();
    estado.conexao.escrita = "writable";
  }
  evento("WebTransport aberto", { ms: Math.round(estado.conexao.ms), maxDatagrama: estado.conexao.maxDatagrama, escrita: estado.conexao.escrita });

  try {
    const uni = await wt.createUnidirectionalStream();
    fluxoRelatorio = uni.getWriter();
  } catch (e) {
    evento("fluxo de relatório não abriu", { erro: e.message });
  }

  wt.closed
    .then((info) => fechou("fechado", info))
    .catch((e) => fechou("caiu", { erro: `${e.name}: ${e.message}` }));

  lerDatagramas(wt.datagrams.readable.getReader());
  pingar();
  $("audio").disabled = false;
  mandarRelatorio("conectou");
}

function fechou(como, info) {
  estado.conexao.ok = false;
  estado.conexao.fechou = { como, t: seg(), info };
  evento(`WebTransport ${como}`, info);
  escritor = null;
  fluxoRelatorio = null;
  mandarPorHttp(relatorio(`webtransport ${como}`));
  $("conectar").disabled = false;
}

function enviar(bytes) {
  if (!escritor) return false;
  escritor.write(bytes).catch(() => {});
  if (document.hidden) estado.oculto.enviadosOculto++;
  return true;
}

let pingador = null;
function pingar() {
  clearInterval(pingador);
  pingador = setInterval(() => {
    const b = new Uint8Array(9);
    b[0] = 0x02;
    new DataView(b.buffer).setFloat64(1, performance.now());
    if (enviar(b)) estado.rtt.enviados++;
  }, 250);
}

async function lerDatagramas(leitor) {
  for (;;) {
    let r;
    try {
      r = await leitor.read();
    } catch {
      return;
    }
    if (r.done) return;
    const d = r.value;
    if (document.hidden) estado.oculto.recebidosOculto++;
    const vista = new DataView(d.buffer, d.byteOffset, d.byteLength);
    if (d[0] === 0x02 && d.length === 9) {
      estado.rtt.voltaram++;
      estado.rtt.amostras.push(performance.now() - vista.getFloat64(1));
      if (estado.rtt.amostras.length > 400) estado.rtt.amostras.shift();
    } else if (d[0] === 0x03 && d.length > 11) {
      // O eco da minha própria voz: decodifica para a medida incluir o decoder.
      const seq = vista.getUint16(1);
      decodificar(0, seq, vista.getFloat64(3), d.subarray(11), true);
    } else if (d[0] === 0x01 && d.length > 13) {
      const de = vista.getUint16(1);
      const seq = vista.getUint16(3);
      estado.voz.recebidos++;
      decodificar(de, seq, vista.getFloat64(5), d.subarray(13), false);
    }
  }
}

// ---- áudio ---------------------------------------------------------------

let ctx = null;
let faixa = null;
let reproducao = null;
let encoder = null;
let seq = 0;
const decoders = new Map(); // id -> { decoder, ultimo, horas: Map(seq -> t) }

async function ligarAudio() {
  $("audio").disabled = true;
  try {
    // As duas tentativas de manter a voz com a tela bloqueada, medidas em
    // 07/10/2026: no Safari do iPhone, com o padrão, o AudioContext vai a
    // `interrupted` ao bloquear e o microfone para de mandar.
    //
    // `?sessao=play-and-record` declara a sessão de áudio da página como uma
    // ligação (a Audio Session API do WebKit), antes de o AudioContext nascer.
    // `?saida=elemento` toca pela tag <audio> em vez de direto pelo destino do
    // WebAudio: o iOS trata a tag como mídia, que pode seguir em segundo plano.
    const pedido = new URLSearchParams(location.search);
    const sessao = pedido.get("sessao");
    if (sessao && navigator.audioSession) {
      navigator.audioSession.type = sessao;
      navigator.audioSession.onstatechange = () => evento(`audioSession ${navigator.audioSession.state}`);
      evento("audioSession declarada", { type: navigator.audioSession.type, state: navigator.audioSession.state });
    }
    ctx = new AudioContext({ sampleRate: 48000, latencyHint: "interactive" });
    await ctx.audioWorklet.addModule("/processadores.js");
    reproducao = new AudioWorkletNode(ctx, "reproducao", { numberOfInputs: 0, outputChannelCount: [2] });
    if (pedido.get("saida") === "elemento") {
      const destino = ctx.createMediaStreamDestination();
      reproducao.connect(destino);
      const elemento = document.createElement("audio");
      elemento.playsInline = true;
      elemento.srcObject = destino.stream;
      elemento.onpause = () => evento("elemento de áudio pausado");
      document.body.append(elemento);
      await elemento.play();
      if (navigator.mediaSession && window.MediaMetadata) {
        navigator.mediaSession.metadata = new MediaMetadata({ title: "SEELE · spike de voz" });
      }
      evento("saída pela tag <audio>");
    } else {
      reproducao.connect(ctx.destination);
    }
    let antes = { blocos: 0, comSom: 0 };
    reproducao.port.onmessage = (e) => {
      estado.voz.faltas = e.data.faltas;
      estado.voz.descartes = e.data.descartes;
      estado.voz.bufferMs = e.data.bufferMs;
      estado.voz.blocos = e.data.blocos;
      estado.voz.comSom = e.data.comSom;
      if (document.hidden) {
        estado.oculto.blocosOculto += e.data.blocos - antes.blocos;
        estado.oculto.comSomOculto += e.data.comSom - antes.comSom;
      }
      antes = { blocos: e.data.blocos, comSom: e.data.comSom };
    };
    ctx.onstatechange = () => evento(`AudioContext ${ctx.state}`);
    await ctx.resume();

    const fonte = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: 1 },
    });
    faixa = fonte.getAudioTracks()[0];
    faixa.onmute = () => evento("microfone mudo pelo sistema");
    faixa.onunmute = () => evento("microfone voltou");
    faixa.onended = () => evento("microfone encerrado");

    const captura = new AudioWorkletNode(ctx, "captura", { numberOfOutputs: 1 });
    // Um nó sem saída ligada pode não ser processado; o ganho zero o mantém
    // puxado sem tocar o microfone de volta no alto-falante.
    const mudo = ctx.createGain();
    mudo.gain.value = 0;
    ctx.createMediaStreamSource(fonte).connect(captura).connect(mudo).connect(ctx.destination);
    captura.port.onmessage = (e) => {
      if (document.hidden) estado.oculto.capturadosOculto++;
      codificar(e.data, performance.now());
    };

    criarEncoder();

    estado.audio = {
      sessao: navigator.audioSession ? navigator.audioSession.type : null,
      saida: pedido.get("saida") === "elemento" ? "elemento" : "destino",
      sampleRate: ctx.sampleRate,
      baseLatencyMs: ctx.baseLatency != null ? Math.round(ctx.baseLatency * 1000) : null,
      outputLatencyMs: ctx.outputLatency != null ? Math.round(ctx.outputLatency * 1000) : null,
      microfone: faixa.getSettings ? faixa.getSettings() : null,
    };
    evento("áudio ligado", estado.audio);
    $("falar").disabled = false;
    $("eco").disabled = false;
  } catch (e) {
    erroDeAudio("ligar o áudio", e);
    $("audio").disabled = false;
  }
}

const horasDeCaptura = new Map();

function criarEncoder() {
  encoder = new AudioEncoder({
    output: (pedaco) => {
      estado.voz.codificados++;
      const opus = new Uint8Array(pedaco.byteLength);
      pedaco.copyTo(opus);
      const s = Math.round(pedaco.timestamp / 20000) & 0xffff;
      const b = new Uint8Array(11 + opus.length);
      const vista = new DataView(b.buffer);
      b[0] = $("eco").checked ? 0x03 : 0x01;
      vista.setUint16(1, s);
      vista.setFloat64(3, horasDeCaptura.get(s) ?? performance.now());
      horasDeCaptura.delete(s);
      b.set(opus, 11);
      if (enviar(b)) estado.voz.enviados++;
    },
    error: (e) => erroDeAudio("encoder", e),
  });
  encoder.configure(ENCODER);
}

// `?sintetico`: um tom de 440 Hz gerado aqui, em quadros de 20 ms, pelo mesmo
// encoder, com eco. Prova o Opus do WebCodecs de ponta a ponta (codificar,
// datagrama, decodificar) num navegador onde não se pode tocar em «Permitir»
// para o microfone. Não mede captura nem saída de som.
function tomSintetico() {
  $("falar").checked = true;
  $("eco").checked = true;
  criarEncoder();
  let fase = 0;
  setInterval(() => {
    const pcm = new Float32Array(QUADRO);
    for (let i = 0; i < QUADRO; i++) pcm[i] = 0.3 * Math.sin((2 * Math.PI * 440 * (fase + i)) / 48000);
    fase += QUADRO;
    codificar(pcm, performance.now());
  }, 20);
  evento("tom sintético ligado");
}

function codificar(pcm, hora) {
  if (!$("falar").checked || !encoder || encoder.state !== "configured") return;
  const s = seq++ & 0xffff;
  horasDeCaptura.set(s, hora);
  if (horasDeCaptura.size > 200) horasDeCaptura.delete(horasDeCaptura.keys().next().value);
  try {
    const dados = new AudioData({
      format: "f32-planar",
      sampleRate: 48000,
      numberOfFrames: QUADRO,
      numberOfChannels: 1,
      timestamp: s * 20000,
      data: pcm,
    });
    encoder.encode(dados);
    dados.close();
  } catch (e) {
    erroDeAudio("encode", e);
  }
}

function decodificar(de, s, hora, opus, ehEco) {
  if (!ctx && !ehEco) return;
  let d = decoders.get(de);
  if (!d) {
    const novo = { ultimo: null, horas: new Map() };
    novo.decoder = new AudioDecoder({
      output: (dados) => saiuDoDecoder(de, novo, dados, ehEco),
      error: (e) => erroDeAudio(`decoder de ${de}`, e),
    });
    novo.decoder.configure(DECODER);
    decoders.set(de, novo);
    d = novo;
  }
  if (!ehEco && d.ultimo !== null) {
    const salto = (s - d.ultimo) & 0xffff;
    if (salto === 0 || salto > 0x8000) estado.voz.fora_de_ordem++;
    else estado.voz.perdidos += salto - 1;
  }
  d.ultimo = s;
  d.horas.set(s, hora);
  if (d.horas.size > 200) d.horas.delete(d.horas.keys().next().value);
  try {
    d.decoder.decode(new EncodedAudioChunk({ type: "key", timestamp: s * 20000, data: opus }));
  } catch (e) {
    erroDeAudio("decode", e);
  }
}

function saiuDoDecoder(de, d, dados, ehEco) {
  const s = Math.round(dados.timestamp / 20000) & 0xffff;
  if (ehEco) {
    const hora = d.horas.get(s);
    if (hora != null) {
      estado.voz.ecoTotal++;
      estado.voz.eco.push(performance.now() - hora);
      if (estado.voz.eco.length > 400) estado.voz.eco.shift();
    }
    dados.close();
    return;
  }
  const pcm = new Float32Array(dados.numberOfFrames);
  dados.copyTo(pcm, { planeIndex: 0, format: "f32-planar" });
  dados.close();
  reproducao.port.postMessage({ id: de, pcm }, [pcm.buffer]);
}

function erroDeAudio(onde, e) {
  const texto = `${onde}: ${e && e.name ? e.name + ": " : ""}${e && e.message ? e.message : e}`;
  estado.voz.erros.push(texto);
  evento("erro de áudio", { texto });
}

// ---- a tela que apaga ----------------------------------------------------

document.addEventListener("visibilitychange", () => {
  const o = estado.oculto;
  if (document.hidden) {
    o.vezes++;
    o.desde = performance.now();
    evento("página oculta", { audio: ctx && ctx.state });
  } else {
    const ms = o.desde ? performance.now() - o.desde : 0;
    o.msTotal += ms;
    o.desde = null;
    evento("página visível de novo", {
      ficouOcultaMs: Math.round(ms),
      audio: ctx && ctx.state,
      webtransport: estado.conexao.ok,
      microfone: faixa && faixa.readyState,
    });
  }
  mandarRelatorio(document.hidden ? "ficou oculta" : "voltou");
});
document.addEventListener("freeze", () => evento("página congelada"));
document.addEventListener("resume", () => evento("página descongelada"));
window.addEventListener("pagehide", () => mandarPorHttp(relatorio("pagehide")));
window.addEventListener("online", () => evento("rede: online"));
window.addEventListener("offline", () => evento("rede: offline"));

// ---- início --------------------------------------------------------------

$("conectar").addEventListener("click", conectar);
$("audio").addEventListener("click", ligarAudio);
$("enviar").addEventListener("click", () => mandarRelatorio("pedido na tela"));

setInterval(() => {
  if (estado.conexao.ok) mandarRelatorio("periódico");
  else pintarMedidas(relatorio("local"));
}, 5000);

(async () => {
  estado.cap = await medirCapacidades();
  pintarCapacidades(estado.cap);
  $("modo").textContent = `${estado.cap.webApp ? "web app (Tela de Início)" : "aba do navegador"} · ${estado.cap.busca}`;
  for (const a of document.querySelectorAll("#testes a")) {
    if (a.getAttribute("href") === location.pathname + location.search) a.setAttribute("aria-current", "page");
  }
  evento("capacidades medidas");
  if (!estado.cap.webtransport) mandarPorHttp(relatorio("sem WebTransport"));
  window.SPIKE.estado = estado; // para a prova automática ler

  // `?auto` conecta sem toque, para medir num navegador que não se pode
  // clicar (o Safari do Simulador do iOS, aberto por `simctl openurl`).
  // `?hash=errado` troca um bit do hash antes: o Safari tem de recusar, ou a
  // conexão que abre não prova nada sobre o pino.
  const pedido = new URLSearchParams(location.search);
  if (pedido.get("hash") === "errado") {
    window.SPIKE.hash[0] ^= 1;
    evento("hash adulterado de propósito");
  }
  if (pedido.has("auto") && estado.cap.webtransport) {
    await conectar();
    if (pedido.has("sintetico")) tomSintetico();
    // Um relatório final depois de alguns pings, para o rtt ter amostras.
    setTimeout(() => mandarRelatorio(`auto${pedido.get("hash") === "errado" ? " · hash errado" : ""}`), 6000);
  }
})();
