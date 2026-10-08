// Candidata A: o Worker que hospeda o motor. O Worker é da origem da página e
// alcança o IndexedDB dela, mas o código do MOD não roda aqui: roda dentro do
// QuickJS, que só vê as funções que `motor-anfitriao.js` importa. Descartável.
"use strict";
importScripts("motor-anfitriao.js");

let motor = null;
self.onmessage = async (e) => {
  const m = e.data;
  if (m.tipo === "iniciar") {
    const t0 = performance.now();
    motor = criarMotor(m.modulo, {
      aoPostar: (json) => { postMessage({ tipo: "fala", json }); return true; },
      aoConsole: (metodo, texto) => postMessage({ tipo: "console", metodo, texto }),
      aoErroNaVolta: (erro) => postMessage({ tipo: "erro-na-volta", erro }),
      agendar: (f, ms, repete) => (repete ? setInterval(f, ms) : setTimeout(f, ms)),
      cancelar: (h) => { clearTimeout(h); clearInterval(h); },
    });
    const r = await motor.subir(m.capacidades, m.fonte);
    postMessage(r.erro
      ? { tipo: "falhou", erro: r.erro }
      : { tipo: "subiu", ms: performance.now() - t0, memoria: motor.memoria() });
  } else if (m.tipo === "entregar") {
    if (motor) motor.entregar(m.json);
  } else if (m.tipo === "memoria") {
    postMessage({ tipo: "memoria", ...(motor ? motor.memoria() : {}) });
  } else if (m.tipo === "encerrar") {
    if (motor) motor.encerrar();
    postMessage({ tipo: "parou" });
    close();
  }
};
