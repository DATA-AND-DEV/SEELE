// A fila de volume do `ui/rtc-web.js` (`criarAFilaDoVolume`), sem navegador:
// a função EXATA do arquivo, carregada num contexto do Node, com canais falsos.
//
//   node apps/seele-app/bancada/fila-do-volume.cjs
//
// O que prova (R4 da revisão, e o defeito medido pelo Codex em 24/10):
// 1. **o teto global vale com 16 canais disputando**: cada um pede a vez de
//    um pedaço de 16 385 bytes ao mesmo tempo, e a soma do que espera nos
//    canais nunca passa do limiar (64 KiB). Antes da reserva, todos passavam
//    juntos e a soma chegava a 262 160;
// 2. **todos acabam**: esvaziando os canais, os 16 escrevem o que tinham;
// 3. **um canal que fecha no meio** rejeita quem dele esperava e não prende
//    crédito: os outros seguem;
// 4. **a fila fechada** rejeita quem espera e quem chega depois;
// 5. **o crédito volta no erro**: uma reserva devolvida sem envio não conta.
//
// Sai com 1 no primeiro que falhar.

"use strict";

const fs = require("fs");
const path = require("path");
const vm = require("vm");

const fonte = fs.readFileSync(path.join(__dirname, "..", "ui", "rtc-web.js"), "utf8");
const contexto = vm.createContext({ setTimeout, clearTimeout, console, performance, crypto: globalThis.crypto });
vm.runInContext(`${fonte}\nthis.criarAFilaDoVolume = criarAFilaDoVolume; this.LIMIAR_DO_VOLUME = LIMIAR_DO_VOLUME;`, contexto);
const { criarAFilaDoVolume, LIMIAR_DO_VOLUME } = contexto;

const PEDACO = 16385;

// Uma fila que deixa alguém pendurado não pode virar uma bancada que nunca
// acaba: dez segundos, e o que ficou é dito.
setTimeout(() => {
  console.error(`fila-do-volume: pendurou; o que já tinha falhado:\n  ${falhas.join("\n  ") || "(nada)"}`);
  process.exit(1);
}, 10_000).unref();
const falhas = [];
const confere = (caso, condicao, detalhe) => {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
};
const espera = (ms) => new Promise((r) => setTimeout(r, ms));

/** Um canal falso: o `bufferedAmount` e os ouvintes do `bufferedamountlow`. */
function canalFalso() {
  const ouvintes = { bufferedamountlow: [], close: [] };
  return {
    bufferedAmount: 0,
    bufferedAmountLowThreshold: 0,
    readyState: "open",
    addEventListener(nome, f) {
      (ouvintes[nome] ??= []).push(f);
    },
    esvaziar() {
      this.bufferedAmount = 0;
      for (const f of ouvintes.bufferedamountlow) f();
    },
    fechar() {
      this.readyState = "closed";
      this.bufferedAmount = 0;
      for (const f of ouvintes.close) f();
    },
  };
}

/** Pede a vez como o `escritaDoCanal` pede, e «manda» o pedaço. */
async function mandarUmPedaco(fila, canal, aoMandar) {
  const reserva = await fila.vez(PEDACO, canal);
  try {
    canal.bufferedAmount += PEDACO;
    aoMandar();
  } finally {
    reserva?.devolver?.();
  }
}

(async () => {
  // ------------------------------------------- 1 e 2. dezesseis canais
  {
    const fila = criarAFilaDoVolume(LIMIAR_DO_VOLUME);
    const canais = Array.from({ length: 16 }, canalFalso);
    for (const canal of canais) fila.juntar(canal);
    const soma = () => canais.reduce((t, c) => t + c.bufferedAmount, 0);
    let maior = 0;
    let mandados = 0;
    const todos = Promise.all(
      canais.map((canal) =>
        mandarUmPedaco(fila, canal, () => {
          mandados += 1;
          maior = Math.max(maior, soma());
        }),
      ),
    );
    await espera(30);
    confere("1", maior <= LIMIAR_DO_VOLUME, `a soma chegou a ${maior}, acima do limiar ${LIMIAR_DO_VOLUME}`);
    confere("1", mandados > 0 && mandados < 16, `com o limiar cheio, ${mandados} de 16 passaram de uma vez`);
    // Esvazia os canais, um de cada vez, até todos mandarem.
    for (let volta = 0; volta < 40 && mandados < 16; volta += 1) {
      for (const canal of canais) if (canal.bufferedAmount > 0) canal.esvaziar();
      await espera(15);
      confere("1", soma() <= LIMIAR_DO_VOLUME, `na volta ${volta}, a soma ${soma()} passou do limiar`);
    }
    await Promise.race([todos, espera(500)]);
    confere("2", mandados === 16, `só ${mandados} de 16 mandaram`);
    console.log(`fila-do-volume 1/2: maior soma ${maior} (limiar ${LIMIAR_DO_VOLUME}), ${mandados}/16 mandados`);
  }

  // ------------------------------------------- 3. um canal fecha no meio
  {
    const fila = criarAFilaDoVolume(LIMIAR_DO_VOLUME);
    const cheio = canalFalso();
    const fechado = canalFalso();
    const outro = canalFalso();
    for (const canal of [cheio, fechado, outro]) fila.juntar(canal);
    cheio.bufferedAmount = LIMIAR_DO_VOLUME;
    let rejeitado = null;
    const doFechado = fila.vez(PEDACO, fechado).then(
      () => "passou",
      (motivo) => {
        rejeitado = motivo;
        return "rejeitado";
      },
    );
    let doOutro = "esperando";
    const pedidoDoOutro = fila.vez(PEDACO, outro).then((reserva) => {
      doOutro = "passou";
      reserva?.devolver?.();
    });
    await espera(20);
    fechado.fechar();
    fila.largar(fechado);
    await espera(20);
    confere("3", (await Promise.race([doFechado, espera(50).then(() => "pendurado")])) === "rejeitado", `quem esperava no canal fechado: ${rejeitado ?? "pendurado"}`);
    cheio.esvaziar();
    await Promise.race([pedidoDoOutro, espera(200)]);
    confere("3", doOutro === "passou", `o outro canal ficou ${doOutro} depois de o fechado sair`);
  }

  // ------------------------------------------- 4. a fila fechada
  {
    const fila = criarAFilaDoVolume(LIMIAR_DO_VOLUME);
    const canal = canalFalso();
    fila.juntar(canal);
    canal.bufferedAmount = LIMIAR_DO_VOLUME;
    const esperando = fila.vez(PEDACO, canal).then(
      () => "passou",
      () => "rejeitado",
    );
    await espera(10);
    fila.fechar("caiu");
    confere("4", (await Promise.race([esperando, espera(50).then(() => "pendurado")])) === "rejeitado", "quem esperava a fila fechada não foi rejeitado");
    const depois = await fila.vez(PEDACO, canal).then(
      () => "passou",
      () => "rejeitado",
    );
    confere("4", depois === "rejeitado", `quem chegou depois do fecho: ${depois}`);
  }

  // ------------------------------------------- 5. o crédito volta no erro
  {
    const fila = criarAFilaDoVolume(LIMIAR_DO_VOLUME);
    const canal = canalFalso();
    fila.juntar(canal);
    // Quatro reservas devolvidas sem enviar (o envio falhou): a quinta passa.
    for (let i = 0; i < 4; i += 1) (await fila.vez(PEDACO, canal))?.devolver?.();
    const quinta = await Promise.race([fila.vez(PEDACO, canal).then(() => "passou"), espera(100).then(() => "presa")]);
    confere("5", quinta === "passou", `depois de quatro reservas devolvidas, a quinta ficou ${quinta}`);
  }

  if (falhas.length) {
    console.error(`fila-do-volume: ${falhas.length} falha(s)\n  ${falhas.join("\n  ")}`);
    process.exit(1);
  }
  console.log("fila-do-volume: ok");
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
