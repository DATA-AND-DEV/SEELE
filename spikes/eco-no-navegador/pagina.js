// O eco no navegador: o que volta pelo microfone de um som que a página toca.
//
// A pergunta (F8, 09/10, para o aviso de fone das duas cascas): o
// `echoCancellation` do `getUserMedia` cancela o som que **a nossa voz** toca?
// A voz do SEELE chega pelo WebTransport e toca pelo Web Audio, e não por um
// `RTCPeerConnection`; se o cancelador do navegador usa essa saída como
// referência varia por motor, e é o que esta página mede.
//
// Como: um tom de 1 kHz tocado pelo Web Audio (o mesmo caminho da voz, até o
// `destination`), sem fone, e o nível de 1 kHz no microfone, com e sem o
// `echoCancellation`. Duas rodadas de cada, alternadas. O retorno é o nível do
// tom no microfone menos o nível de 1 kHz no silêncio de antes.
//
// O veredito, por motor:
// - **sem laço** se o retorno sem cancelador ficar abaixo de 20 dB: o tom não
//   chegou ao microfone (fone, volume baixo, a saída em outro lugar), e a
//   medida não diz nada;
// - **cancela** se, com laço, o cancelador tirar 20 dB ou mais;
// - **cancela em parte** de 6 a 20 dB; **não cancela** abaixo de 6.

"use strict";

const FREQUENCIA = 1000;
const GANHO_DO_TOM = 0.25;
const SILENCIO_MS = 1500;
// O cancelador leva um tempo para aprender o caminho; só a segunda metade do
// tom conta.
const APRENDER_MS = 2000;
const MEDIR_MS = 2000;
const AMOSTRA_MS = 50;
const RODADAS = [false, true, false, true];

const espera = (ms) => new Promise((pronto) => setTimeout(pronto, ms));

function escrever(texto) {
  const linha = document.createElement("div");
  linha.textContent = texto;
  document.getElementById("saida").append(linha);
}

/** O nível de 1 kHz no analisador agora, em dB (o maior dos bins vizinhos). */
function nivelEm1k(analisador, contexto, quadro) {
  analisador.getFloatFrequencyData(quadro);
  const largura = contexto.sampleRate / analisador.fftSize;
  const centro = Math.round(FREQUENCIA / largura);
  let maior = -Infinity;
  for (let i = centro - 3; i <= centro + 3; i++) maior = Math.max(maior, quadro[i]);
  return maior;
}

/** A média dos níveis amostrados durante `ms`. */
async function amostrar(analisador, contexto, ms) {
  const quadro = new Float32Array(analisador.frequencyBinCount);
  const niveis = [];
  for (let feito = 0; feito < ms; feito += AMOSTRA_MS) {
    await espera(AMOSTRA_MS);
    const nivel = nivelEm1k(analisador, contexto, quadro);
    if (Number.isFinite(nivel)) niveis.push(nivel);
  }
  return niveis.length ? niveis.reduce((a, b) => a + b, 0) / niveis.length : -Infinity;
}

/** Uma rodada: o silêncio, o tom e o que voltou, com o cancelador pedido assim. */
async function rodada(cancelador) {
  const fluxo = await navigator.mediaDevices.getUserMedia({
    audio: { echoCancellation: cancelador, noiseSuppression: false, autoGainControl: false },
  });
  const [faixa] = fluxo.getAudioTracks();
  const aplicado = faixa.getSettings().echoCancellation;
  const contexto = new AudioContext();
  await contexto.resume();
  const analisador = contexto.createAnalyser();
  analisador.fftSize = 8192;
  analisador.smoothingTimeConstant = 0;
  contexto.createMediaStreamSource(fluxo).connect(analisador);
  const tom = contexto.createOscillator();
  tom.frequency.value = FREQUENCIA;
  const volume = contexto.createGain();
  volume.gain.value = 0;
  tom.connect(volume).connect(contexto.destination);
  tom.start();
  try {
    await espera(500);
    const silencio = await amostrar(analisador, contexto, SILENCIO_MS);
    volume.gain.value = GANHO_DO_TOM;
    await espera(APRENDER_MS);
    const comTom = await amostrar(analisador, contexto, MEDIR_MS);
    return {
      pedido: cancelador,
      aplicado: aplicado ?? null,
      silencio_db: Math.round(silencio * 10) / 10,
      tom_db: Math.round(comTom * 10) / 10,
      retorno_db: Math.round((comTom - silencio) * 10) / 10,
    };
  } finally {
    volume.gain.value = 0;
    tom.stop();
    for (const f of fluxo.getTracks()) f.stop();
    await contexto.close();
  }
}

/** O veredito, pelas regras do cabeçalho. */
function veredito(rodadas) {
  const media = (pedido) => {
    const deste = rodadas.filter((r) => r.pedido === pedido).map((r) => r.retorno_db);
    return deste.reduce((a, b) => a + b, 0) / deste.length;
  };
  const sem = media(false);
  const com = media(true);
  const tirou = Math.round((sem - com) * 10) / 10;
  let resposta;
  if (!(sem >= 20)) resposta = "sem laço: o tom não chegou ao microfone, e a medida não diz nada";
  else if (tirou >= 20) resposta = "cancela";
  else if (tirou >= 6) resposta = "cancela em parte";
  else resposta = "não cancela";
  return { retorno_sem_db: Math.round(sem * 10) / 10, retorno_com_db: Math.round(com * 10) / 10, tirou_db: tirou, resposta };
}

async function medir() {
  document.getElementById("medir").disabled = true;
  const rodadas = [];
  try {
    for (const cancelador of RODADAS) {
      escrever(`rodada com echoCancellation ${cancelador ? "ligado" : "desligado"}…`);
      const r = await rodada(cancelador);
      rodadas.push(r);
      escrever(`  pedido ${r.pedido}, aplicado ${r.aplicado}: retorno ${r.retorno_db} dB (silêncio ${r.silencio_db}, tom ${r.tom_db})`);
    }
    const final = { agente: navigator.userAgent, rodadas, veredito: veredito(rodadas) };
    escrever(`VEREDITO: ${final.veredito.resposta} (sem ${final.veredito.retorno_sem_db} dB, com ${final.veredito.retorno_com_db} dB, tirou ${final.veredito.tirou_db} dB)`);
    await fetch("resultado", { method: "POST", body: JSON.stringify(final) });
    escrever("o resultado foi para o Mac. Pode fechar.");
  } catch (falha) {
    escrever(`FALHOU: ${falha?.name ?? ""} ${falha?.message ?? falha}`);
    await fetch("resultado", { method: "POST", body: JSON.stringify({ agente: navigator.userAgent, rodadas, falha: String(falha) }) }).catch(() => {});
  }
}

document.getElementById("medir").addEventListener("click", medir);
