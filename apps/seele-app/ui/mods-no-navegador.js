// Os MODs no navegador (o celular e o PC, D-web-3): o motor do desktop, num
// lugar sem a origem da página.
//
// ADR 0057, emenda de 08/10/2026 (D-web-1), na forma que o spike
// `spikes/mods-no-navegador/` recomendou e a orquestradora aceitou: **AB**. O
// QuickJS do desktop, compilado para `.wasm` (`crates/seele-motor-web`), roda
// num Worker que um `iframe` opaco cria (`sandbox="allow-scripts"`, sem
// `allow-same-origin`). Há um `iframe` só, e um Worker por MOD.
//
// # Por que duas paredes
//
// **A primeira é o motor.** O código do MOD roda dentro do QuickJS, e um
// contexto de QuickJS não tem `fetch`, `indexedDB` nem `postMessage`: tem o que
// o prelúdio põe lá, e o prelúdio é o mesmo texto do desktop.
//
// **A segunda é o `iframe`.** Se um defeito do motor um dia deixar o MOD sair
// dele, o MOD cai num Worker de origem opaca, sem o armazenamento da página (é
// lá que mora a `CryptoKey` da pessoa) e sob a CSP do `iframe`, que não deixa
// conectar a nada. Um Worker de `blob:` criado **pela página** herdaria a
// origem dela, e foi esse o defeito que tirou o Worker do desktop (o
// `executorNativo`, em `mods-runtime.js`).
//
// # A forma é a do `executorNativo`
//
// Quatro verbos. O `entregar` devolve a recusa, e o `encerrou` tem prazo e
// conclusão tardia. Quem chama não sabe qual dos dois está do outro lado: o
// `executorDoMod`, em `mods-runtime.js`, escolhe pelo lugar.
//
// # No desktop, nada
//
// Este arquivo é carregado nas duas cascas, e no desktop só declara. O ouvinte
// de `message` e o `iframe` nascem na primeira subida de um MOD por aqui.
//
// # O que atravessa
//
// Página → `iframe`: `preparar` (o motor, uma vez), `subir`,
// `entregar`, `lido` e `encerrar`, cada um com o número da instância.
// `iframe` → página: o que o Worker disse, com o número que **o `iframe`**
// põe, e não o que o Worker escreveu. Um Worker que mentisse o número falaria
// em nome de outro MOD.

/** O que a página baixa para os executores, ao lado dela. */
const ARQUIVOS_DOS_EXECUTORES = Object.freeze({
  iframe: "executor-de-mod.html",
  motor: "seele_motor_web.wasm",
});

/**
 * Os tetos das filas, os mesmos do desktop (`executor.rs`: `MENSAGENS_NA_FILA`
 * e `BYTES_NA_FILA`). A de entrada é medida daqui; a de saída, no Worker,
 * que é quem recusa o `postar` do MOD.
 */
const FILA_DO_EXECUTOR = Object.freeze({ mensagens: 64, bytes: 256 * 1024 });

/** Quanto a página espera o `iframe` dos executores carregar. */
const PRAZO_DO_IFRAME_MS = 15000;

/** Quanto o `encerrou` espera a confirmação: o mesmo prazo do nativo. */
const PRAZO_DA_PARADA_MS = 2000;

/**
 * O `iframe` dos executores, um por página.
 *
 * `pronto` é a promessa do `iframe` preparado. Uma preparação que falhou não
 * fica guardada: o MOD seguinte tenta de novo, e a falha é dita a cada um.
 */
const executoresDoNavegador = {
  iframe: null,
  pronto: null,
  ouvindo: false,
  /** número da instância → quem ouve o que ela diz */
  ouvintes: new Map(),
  proximo: 0,
};

/** Os bytes de um texto em UTF-8: a conta do desktop é `json.len()`. */
const bytesDoTexto = (texto) => new TextEncoder().encode(texto).length;

function mandarAoIframe(mensagem, transferir) {
  executoresDoNavegador.iframe.contentWindow.postMessage(mensagem, "*", transferir ?? []);
}

/**
 * Sobe o `iframe` e entrega a ele o motor, uma vez por página.
 *
 * O `iframe` não busca nada sozinho: com origem opaca, um `fetch` dele para a
 * origem da página seria de outra origem. A página baixa e entrega.
 */
function prepararOsExecutoresDoNavegador() {
  if (executoresDoNavegador.pronto) return executoresDoNavegador.pronto;
  const pronto = (async () => {
    const baixar = async (nome, como) => {
      const resposta = await fetch(nome);
      if (!resposta.ok) throw new Error(`${nome} não veio (${resposta.status})`);
      return resposta[como]();
    };
    const motor = await baixar(ARQUIVOS_DOS_EXECUTORES.motor, "arrayBuffer");
    if (!executoresDoNavegador.ouvindo) {
      window.addEventListener("message", ouvirOsExecutores);
      executoresDoNavegador.ouvindo = true;
    }
    const iframe = document.createElement("iframe");
    // **Sem `allow-same-origin`, nunca.** Com ele, o `iframe` volta a ter a
    // origem da página, e a segunda parede deixa de existir.
    iframe.sandbox = "allow-scripts";
    iframe.hidden = true;
    iframe.setAttribute("aria-hidden", "true");
    iframe.src = ARQUIVOS_DOS_EXECUTORES.iframe;
    executoresDoNavegador.iframe = iframe;
    await new Promise((preparado, falhou) => {
      executoresDoNavegador.aoPreparar = { preparado, falhou };
      // Um `iframe` que não carrega não diz nada: sem o prazo, todo MOD
      // esperaria para sempre.
      setTimeout(() => falhou(new Error(`${ARQUIVOS_DOS_EXECUTORES.iframe} não respondeu`)), PRAZO_DO_IFRAME_MS);
      document.body.append(iframe);
    });
    // O motor vai uma vez, e o `iframe` o compila uma vez: cada Worker recebe
    // o módulo já compilado.
    await new Promise((preparado, falhou) => {
      executoresDoNavegador.aoPreparar = { preparado, falhou };
      mandarAoIframe({ tipo: "preparar", motor }, [motor]);
    });
    executoresDoNavegador.aoPreparar = null;
  })();
  executoresDoNavegador.pronto = pronto;
  pronto.catch(() => {
    if (executoresDoNavegador.pronto !== pronto) return;
    executoresDoNavegador.pronto = null;
    executoresDoNavegador.iframe?.remove();
    executoresDoNavegador.iframe = null;
  });
  return pronto;
}

/** O que o `iframe` dos executores diz à página. */
function ouvirOsExecutores(evento) {
  const iframe = executoresDoNavegador.iframe;
  // **Só o `iframe` fala aqui.** A origem dele é `null`, e por isso a conferência
  // é pela janela, e não pela origem.
  if (!iframe || evento.source !== iframe.contentWindow) return;
  const m = evento.data;
  if (!m || typeof m !== "object") return;
  if (m.tipo === "carregou" || m.tipo === "preparado") {
    executoresDoNavegador.aoPreparar?.preparado();
    return;
  }
  if (m.tipo === "nao-preparou") {
    executoresDoNavegador.aoPreparar?.falhou(new Error(String(m.erro)));
    return;
  }
  executoresDoNavegador.ouvintes.get(m.n)?.(m);
}

/**
 * O executor dos MODs no navegador, com a forma do `executorNativo`.
 *
 * `iniciar` recebe, no lugar do código, `{ fonte, capacidades }`: no desktop o
 * lado nativo lê o pacote e as capacidades pelo manifesto; aqui quem as dá é a
 * ponte, com o mesmo `capacidades_da_api` do `seele-proto`.
 */
function executorNoNavegador(id, geracao, hash) {
  let numero = null;
  let parada = null;
  let resolverAParada = null;
  // O que fazer quando a parada for confirmada, inclusive depois do prazo.
  let aoConcluir = null;
  let parou = false;
  // Pediram a parada antes de o número existir: o MOD não sobe.
  let pediuAParada = false;
  // A fila de entrada: o que foi entregue e o Worker ainda não leu.
  const entrada = { mensagens: 0, bytes: 0, recusadas: 0 };
  // O que o MOD disse, em ordem: uma coisa de cada vez, como a colheita do
  // nativo, que lê mensagens, falhas e a parada do mesmo canal.
  let atendendo = Promise.resolve();
  // Uma etapa que lança não pode parar as seguintes: a fila seguiria de pé e
  // muda, e a parada nunca concluiria.
  const emOrdem = (fazer) => {
    atendendo = atendendo.then(fazer).catch((falha) => {
      console.warn(`MOD ${id}: o executor do navegador`, falha);
    });
  };
  // A subida já foi dita? Antes dela, uma falha é da subida e é dita na hora:
  // é por ela que a casca diz «não carregou» em vez de «falhou rodando».
  let subiu = false;

  const concluir = () => {
    parou = true;
    resolverAParada?.();
    if (numero !== null) executoresDoNavegador.ouvintes.delete(numero);
    const terminar = aoConcluir;
    aoConcluir = null;
    terminar?.();
  };

  return {
    nome: "navegador",
    async iniciar(codigo, aoReceber, aoFalhar) {
      parada = new Promise((resolve) => {
        resolverAParada = resolve;
      });
      const fonte = codigo?.fonte;
      const capacidades = codigo?.capacidades;
      if (typeof fonte !== "string" || !Array.isArray(capacidades)) {
        aoFalhar(`MOD ${id}: o código chegou sem fonte ou sem capacidades`);
        concluir();
        return;
      }
      try {
        await prepararOsExecutoresDoNavegador();
      } catch (falha) {
        // **Dito, e não engolido.** Sem o motor nenhum MOD sobe, e a frase
        // tem de dizer que é o executor, e não o MOD.
        aoFalhar(`o executor do navegador não subiu: ${falha.message}`);
        concluir();
        return;
      }
      if (pediuAParada) {
        concluir();
        return;
      }
      numero = ++executoresDoNavegador.proximo;
      await new Promise((aoSubir) => {
        const dizerQueSubiu = () => {
          subiu = true;
          aoSubir();
        };
        executoresDoNavegador.ouvintes.set(numero, (m) => {
          switch (m.tipo) {
            case "mensagem": {
              const { corpo, bytes } = m;
              emOrdem(async () => {
                try {
                  let valor;
                  try {
                    valor = JSON.parse(corpo);
                  } catch {
                    aoFalhar("o MOD mandou o que não é JSON");
                    return;
                  }
                  try {
                    await aoReceber(valor);
                  } catch (falha) {
                    aoFalhar(String(falha?.message ?? falha));
                  }
                } finally {
                  // O lugar volta **depois** de atendida, como no nativo, em
                  // que é a colheita que devolve o crédito.
                  if (!parou) mandarAoIframe({ tipo: "lido", n: numero, mensagens: 1, bytes });
                }
              });
              break;
            }
            case "entrada-lida":
              entrada.mensagens = Math.max(0, entrada.mensagens - 1);
              entrada.bytes = Math.max(0, entrada.bytes - m.bytes);
              break;
            case "falhou":
            case "interrompido": {
              // `falhou` e `interrompido` pedem frases diferentes, como no
              // nativo: um é o MOD com defeito, o outro é o produto parando-o.
              const frase = m.tipo === "interrompido" ? "interrompido pelo produto" : m.corpo;
              // O crédito do aviso volta depois de dito, como a colheita do
              // desktop o devolve: um MOD que falha num laço não enche a página.
              const dizer = () => {
                aoFalhar(frase);
                if (!parou) mandarAoIframe({ tipo: "aviso-lido", n: numero });
              };
              if (subiu) emOrdem(dizer);
              else dizer();
              break;
            }
            case "console": {
              // **O registro do MOD, no celular.** No desktop a linha vai ao
              // `seele.log`; aqui, até o canal de diagnóstico ser decidido (ADR
              // 0057, «O que este ADR não decide»), ela vai ao console da
              // página, onde o inspetor do Safari a lê. O motor já a passou
              // pelo balde e pelo corte do desktop.
              const metodo = ["debug", "info", "warn", "error"].includes(m.nivel) ? m.nivel : "info";
              const seguradas = m.suprimidas > 0 ? ` (e ${m.suprimidas} seguradas antes)` : "";
              console[metodo](`MOD ${id}: ${m.texto}${seguradas}`);
              break;
            }
            case "subiu":
              dizerQueSubiu();
              break;
            case "parou":
              // O que o MOD disse antes de parar ainda chega, e só então a
              // parada conclui.
              dizerQueSubiu();
              emOrdem(concluir);
              break;
            default:
              break;
          }
        });
        mandarAoIframe({ tipo: "subir", n: numero, fonte, capacidades, mod: id, geracao, hash });
      });
    },
    /**
     * Manda uma mensagem para dentro, **devolvendo a recusa**, como o nativo.
     */
    entregar(mensagem) {
      if (numero === null || parou) {
        return Promise.reject(new Error("instancia-nao-reservada"));
      }
      const json = JSON.stringify(mensagem);
      const bytes = bytesDoTexto(json);
      if (
        entrada.mensagens >= FILA_DO_EXECUTOR.mensagens ||
        entrada.bytes + bytes > FILA_DO_EXECUTOR.bytes
      ) {
        entrada.recusadas += 1;
        return Promise.reject(new Error("a fila de entrada do executor está cheia"));
      }
      entrada.mensagens += 1;
      entrada.bytes += bytes;
      mandarAoIframe({ tipo: "entregar", n: numero, json, bytes });
      return Promise.resolve();
    },
    pedirEncerramento() {
      if (parou) return;
      // **Antes do número, a parada fica anotada.** O nativo não precisa disto,
      // porque reservar não espera nada; aqui a primeira subida espera baixar o
      // motor, e um MOD encerrado nesse meio-tempo subiria depois.
      if (numero === null) {
        pediuAParada = true;
        return;
      }
      mandarAoIframe({ tipo: "encerrar", n: numero });
    },
    /** Espera a confirmação, com prazo, e avisa quando ela chega tarde. */
    async encerrou(quandoConcluir) {
      if (parou) return true;
      aoConcluir = quandoConcluir ?? null;
      const confirmou = await Promise.race([
        parada.then(() => true),
        new Promise((resolve) => setTimeout(() => resolve(false), PRAZO_DA_PARADA_MS)),
      ]);
      if (confirmou) aoConcluir = null;
      return confirmou;
    },
    /** O que ficou pendente deste lado: para a bancada. */
    pendencias() {
      return { numero, parou, entrada: { ...entrada } };
    },
  };
}
