// O que duas páginas que não se ligaram sabem do próprio WebRTC, dito quando a
// bancada reprova.
//
// # Por que este arquivo existe
//
// Na primeira execução do `ci.yml` em meses (43264412497, 12/11/2026), quatro
// bancadas passaram no Chromium e reprovaram no WebKit do runner de Linux, todas
// no mesmo ponto: duas páginas que deviam se ligar por WebRTC, sem TURN, na
// mesma máquina. O que cada uma disse foi o prazo —
//
//     page.waitForFunction: Timeout 20000ms exceeded.
//
// — e a `compartilhar-pessoal-web.cjs` nem isso: pendurou até o teto de cinco
// minutos do passo. Nenhuma disse em que estado o ICE ficou, nem que
// candidatos cada lado juntou e recebeu. A hipótese que sobrou (o WebKit de
// Linux anuncia o candidato de host por um nome mDNS, `<uuid>.local`, que o
// outro lado não resolve num runner) **não foi medida**, e com o que as
// bancadas diziam não havia como medi-la.
//
// O produto já sabe parte disso: o enlace tem `diagnostico`, com os dois
// estados e a contagem dos candidatos por tipo. Só que a contagem diz «host» e
// não diz o endereço, e a pergunta aqui é o endereço. Por isso o registro fica
// na bancada, em volta do `RTCPeerConnection` da página, e guarda a linha
// inteira de cada candidato.
//
// # O que ele faz
//
// - `registrarRtc(alvo)`: antes de qualquer script da página (um contexto ou
//   uma página do Playwright), troca o `RTCPeerConnection` por uma subclasse
//   que anota cada conexão criada, os candidatos locais que ela junta
//   (`icecandidate`), os remotos que recebe (`addIceCandidate`) e cada mudança
//   de estado, com o instante. Não muda o que a conexão faz.
// - `relatoRtc(pagina)`: o que foi anotado, com os estados de agora e os pares
//   de candidatos que o `getStats()` conhece. Nunca rejeita: uma página que já
//   fechou responde com o erro, e não derruba o relato da outra.
// - `relatarRtc(rotulo, paginas)`: escreve o relato de cada página no stderr.
// - `comPrazo(promessa, ms, frase)`: a promessa, ou um erro com a frase quando
//   o prazo vence — para a espera que não tinha prazo nenhum.
//
// Não é bancada: não tem asserção própria. O `check_runtime.rs` o nomeia entre
// os arquivos da pasta que não são bancada.

/** Roda na página, antes dos scripts dela. */
function registrarNaPagina() {
  const Original = globalThis.RTCPeerConnection;
  if (typeof Original !== "function" || globalThis.__seeleRtc) return;
  const conexoes = [];
  globalThis.__seeleRtc = conexoes;
  const agora = () => Math.round(performance.now());
  globalThis.RTCPeerConnection = class extends Original {
    constructor(...argumentos) {
      super(...argumentos);
      const registro = { pc: this, criada: agora(), locais: [], remotos: [], estados: [] };
      conexoes.push(registro);
      const anotar = () => registro.estados.push(
        `+${agora() - registro.criada} ms: ice ${this.iceConnectionState}, conexão ${this.connectionState}, ` +
        `coleta ${this.iceGatheringState}, sinalização ${this.signalingState}`,
      );
      for (const evento of ["iceconnectionstatechange", "connectionstatechange", "icegatheringstatechange", "signalingstatechange"]) {
        this.addEventListener(evento, anotar);
      }
      this.addEventListener("icecandidate", ({ candidate }) => {
        registro.locais.push(candidate?.candidate || "(fim da coleta)");
      });
      this.addEventListener("icecandidateerror", (erro) => {
        registro.locais.push(`(erro de candidato ${erro.errorCode}: ${erro.errorText} em ${erro.url})`);
      });
    }

    addIceCandidate(candidato, ...resto) {
      const registro = conexoes.find((uma) => uma.pc === this);
      registro?.remotos.push(candidato ? (candidato.candidate ?? String(candidato)) || "(fim dos candidatos)" : "(fim dos candidatos)");
      return super.addIceCandidate(candidato, ...resto);
    }
  };
  globalThis.__seeleRelatoRtc = () => Promise.all(conexoes.map(async (registro, indice) => {
    const pares = [];
    try {
      const medidas = await registro.pc.getStats();
      const candidatos = new Map();
      medidas.forEach((medida) => {
        if (medida.type === "local-candidate" || medida.type === "remote-candidate") {
          candidatos.set(medida.id, `${medida.candidateType} ${medida.protocol} ${medida.address ?? medida.ip ?? "?"}:${medida.port}`);
        }
      });
      medidas.forEach((medida) => {
        if (medida.type !== "candidate-pair") return;
        pares.push(
          `${medida.state}${medida.nominated ? ", nomeado" : ""}: ` +
          `${candidatos.get(medida.localCandidateId) ?? medida.localCandidateId} → ` +
          `${candidatos.get(medida.remoteCandidateId) ?? medida.remoteCandidateId}`,
        );
      });
    } catch (falha) {
      pares.push(`getStats recusou: ${falha}`);
    }
    return {
      conexao: indice,
      iceConnectionState: registro.pc.iceConnectionState,
      connectionState: registro.pc.connectionState,
      iceGatheringState: registro.pc.iceGatheringState,
      signalingState: registro.pc.signalingState,
      candidatosLocais: registro.locais,
      candidatosRemotos: registro.remotos,
      paresDeCandidatos: pares,
      estados: registro.estados,
    };
  }));
}

/** Instala o registro num contexto ou numa página, antes de qualquer `goto`. */
async function registrarRtc(alvo) {
  await alvo.addInitScript(registrarNaPagina);
}

/** O relato de uma página. Nunca rejeita. */
async function relatoRtc(pagina) {
  try {
    return await pagina.evaluate(async () => {
      if (typeof globalThis.__seeleRelatoRtc !== "function") {
        return { semRegistro: "a página não tem o registro do WebRTC: `registrarRtc` não rodou antes do `goto`, ou ela não tem RTCPeerConnection" };
      }
      const conexoes = await globalThis.__seeleRelatoRtc();
      return conexoes.length ? conexoes : { nenhumaConexao: "a página não criou nenhum RTCPeerConnection" };
    });
  } catch (falha) {
    return { semRelato: String(falha) };
  }
}

/**
 * Escreve no stderr o relato de cada página de `paginas` (`{nome: página}`).
 * É o que se chama no `catch` de uma bancada, antes de fechar o navegador.
 */
async function relatarRtc(rotulo, paginas) {
  for (const [nome, pagina] of Object.entries(paginas)) {
    if (!pagina) continue;
    console.error(`diagnóstico do WebRTC — ${rotulo}, página ${nome}:\n${JSON.stringify(await relatoRtc(pagina), null, 1)}`);
  }
}

/** `promessa`, ou um erro com `frase` quando `ms` vencem antes dela. */
function comPrazo(promessa, ms, frase) {
  let relogio;
  const prazo = new Promise((_, recusar) => {
    relogio = setTimeout(() => recusar(new Error(`${frase} (prazo de ${ms / 1000} s)`)), ms);
  });
  return Promise.race([promessa, prazo]).finally(() => clearTimeout(relogio));
}

module.exports = { registrarRtc, relatoRtc, relatarRtc, comPrazo, registrarNaPagina };
