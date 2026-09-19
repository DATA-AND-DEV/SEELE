// As corridas do ciclo de vida de um MOD, contra o código de verdade.
//
// Etapa E2 e as revisões de `6cc58c1` e `fd1de3a`. Ele carrega
// `ui/mods-runtime.js` num contexto de VM com transporte simulado, e controla a
// **ordem dos eventos** — que é a única coisa que separa um caminho correto de
// um que só parece correto.
//
// # Por que em `bancada/` e não em `testes/`
//
// `testes/` guarda **vetores**: bytes que são o contrato, conferidos um a um,
// e declarados `-text` no `.gitattributes` para o Git do Windows não os
// converter. Este arquivo não é um vetor — é um instrumento —, e pôr um
// instrumento lá faria aquela regra dizer de si mesma o que não é verdade.
//
// # Por que aqui e não na bateria do Rust
//
// Porque o que se mede é comportamento de JavaScript com ordem de mensagens
// controlada, e a bateria do Rust lê arquivos. Um guarda de texto prova que a
// linha existe; este prova que a ordem funciona.
//
// E não entra em `cargo test` porque `cargo test` não pode depender de Node
// estar instalado: quem roda a bateria deste repositório é quem compila o
// produto, e o produto não precisa de Node para nada. Ele é chamado por
// `cargo xtask check-runtime`, que é onde as ferramentas moram.
//
// Sai com 1 no primeiro caso que falhar, dizendo qual.

const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const raiz = path.resolve(__dirname, "..");
const fonte = fs.readFileSync(path.join(raiz, "ui/mods-runtime.js"), "utf8");

/** Uma bancada: o runtime carregado com ponte e relógio de mentira. */
function bancada() {
  let ouvinte = null;
  let removidos = 0;
  const prazos = [];
  const chamadas = [];
  const respostas = new Map();
  const contexto = vm.createContext({
    console,
    setTimeout: (fn) => {
      prazos.push(fn);
      return prazos.length;
    },
    listen: async (_nome, callback) => {
      ouvinte = callback;
      return () => {
        removidos += 1;
      };
    },
    invoke: (cmd, args) => {
      chamadas.push({ cmd, args });
      const combinada = respostas.get(cmd);
      if (typeof combinada === "function") return Promise.resolve(combinada(args));
      return Promise.resolve(combinada ?? null);
    },
  });
  vm.runInContext(
    `${fonte}\nglobalThis.api = { executorNativo, executorDeWorker, InstanciaDeMod, ESTADOS_DE_MOD, recursosDePe };`,
    contexto,
  );
  return {
    api: contexto.api,
    evento: (payload) => ouvinte?.({ payload }),
    responder: (cmd, valor) => respostas.set(cmd, valor),
    prazos,
    chamadas,
    removidos: () => removidos,
  };
}

const volta = () => new Promise((r) => setImmediate(r));
const falhas = [];
function confere(caso, condicao, detalhe) {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
}

// ---------------------------------------------------------------------------

/// **Uma fala de instância antiga não é aceita durante a subida.**
///
/// Reprodução de `fd1de3a`: enquanto o número era `null`, o ouvinte casava pelo
/// nome do MOD. A instância 99 mandava mensagem e `parou`; a mensagem era
/// aceita, a resposta se perdia, e o `parou` dela confirmava o encerramento da
/// 100.
async function falaAntigaNaoEntraDuranteASubida() {
  const caso = "fala antiga durante a subida";
  const b = bancada();
  let reservar;
  b.responder("mod_nativo_reservar", () => new Promise((r) => { reservar = r; }));
  b.responder("mod_nativo_colher", () => []);

  const e = b.api.executorNativo("autor/mod", 7, "hash");
  let recebidas = 0;
  const subindo = e.iniciar("", () => { recebidas += 1; }, () => {});
  await volta();

  // A instância antiga fala na janela em que a nova ainda não tem número.
  b.evento({ id: "autor/mod", geracao: 7, instancia: 99, parou: false });
  b.evento({ id: "autor/mod", geracao: 7, instancia: 99, parou: true });
  await volta();

  confere(caso, recebidas === 0, `aceitou ${recebidas} mensagem(ns) de outra instância`);

  reservar(100);
  await subindo;

  // E o `parou` da antiga não confirma o encerramento da nova.
  const fim = e.encerrou();
  await volta();
  b.prazos.forEach((fn) => fn());
  const confirmou = await fim;
  confere(caso, confirmou === false, "o `parou` de outra instância confirmou esta parada");

  // A ativação vem depois da reserva, e é ela que libera a execução.
  const ordem = b.chamadas.map((c) => c.cmd);
  confere(
    caso,
    ordem.indexOf("mod_nativo_reservar") < ordem.indexOf("mod_nativo_ativar"),
    `a ordem das etapas está errada: ${ordem.join(", ")}`,
  );
}

/// **A confirmação que chega depois do prazo conclui o encerramento.**
///
/// Reprodução de `fd1de3a`: o `parou` tardio só resolvia a promessa antiga —
/// não soltava o ouvinte, não atualizava a instância, e a promessa já
/// concluída impedia nova tentativa.
async function confirmacaoTardiaConclui() {
  const caso = "confirmação depois do prazo";
  const b = bancada();
  b.responder("mod_nativo_reservar", () => 101);
  b.responder("mod_nativo_colher", () => []);

  const e = b.api.executorNativo("autor/mod", 8, "hash");
  await e.iniciar("", () => {}, () => {});
  const instancia = new b.api.InstanciaDeMod("autor/mod", "hash", 8, e);

  const fechando = instancia.encerrar();
  await volta();
  b.prazos.forEach((fn) => fn());
  const primeira = await fechando;
  confere(caso, primeira === false, "o prazo vencido devolveu confirmação");
  confere(
    caso,
    instancia.estado === b.api.ESTADOS_DE_MOD.encerrando,
    `estado depois do prazo: ${instancia.estado}`,
  );

  // Agora a confirmação chega, tarde.
  b.evento({ id: "autor/mod", geracao: 8, instancia: 101, parou: true });
  await volta();
  await volta();

  confere(
    caso,
    instancia.estado === b.api.ESTADOS_DE_MOD.encerrada,
    `a confirmação tardia não concluiu: estado ${instancia.estado}`,
  );
  confere(caso, b.removidos() === 1, `o ouvinte não foi solto: ${b.removidos()}`);
  confere(
    caso,
    b.api.recursosDePe(new Map([[1, instancia]])).length === 0,
    "o diagnóstico continua acusando sobra de um MOD que já parou",
  );
}

/// **Uma segunda tentativa de encerrar não devolve a resposta velha.**
///
/// Reprodução de `fd1de3a`: «a promessa de `encerrar` já concluída impede nova
/// tentativa». Depois de um prazo vencido, chamar de novo devolvia o mesmo
/// `false` guardado, sem nunca mais olhar — e se a confirmação chegasse nesse
/// meio-tempo, não havia por onde saber.
async function umaSegundaTentativaDeEncerrarValeDeNovo() {
  const caso = "segunda tentativa de encerrar";
  const b = bancada();
  b.responder("mod_nativo_reservar", () => 103);
  b.responder("mod_nativo_colher", () => []);

  const e = b.api.executorNativo("autor/mod", 10, "hash");
  await e.iniciar("", () => {}, () => {});
  const instancia = new b.api.InstanciaDeMod("autor/mod", "hash", 10, e);

  const primeira = instancia.encerrar();
  await volta();
  b.prazos.forEach((fn) => fn());
  confere(caso, (await primeira) === false, "o prazo vencido devolveu confirmação");

  // A confirmação chega, e agora a segunda tentativa tem de vê-la.
  b.evento({ id: "autor/mod", geracao: 10, instancia: 103, parou: true });
  await volta();
  const segunda = await instancia.encerrar();
  confere(
    caso,
    segunda === true,
    "a segunda tentativa devolveu a resposta velha em vez de olhar de novo",
  );
}

/// **A janela colhe, e é a colheita que devolve o crédito.**
///
/// O aviso não carrega mensagem: ele diz «há o que colher». Com a janela
/// parada, nada se acumula do lado dela antes de qualquer callback rodar.
async function aJanelaColheEmVezDeReceber() {
  const caso = "colher em vez de receber";
  const b = bancada();
  b.responder("mod_nativo_reservar", () => 102);
  let colheitas = 0;
  b.responder("mod_nativo_colher", () => {
    colheitas += 1;
    return colheitas === 1 ? [{ tipo: "mensagem", corpo: '{"oi":1}' }] : [];
  });

  const e = b.api.executorNativo("autor/mod", 9, "hash");
  const vistas = [];
  await e.iniciar("", (m) => vistas.push(m), () => {});

  b.evento({ id: "autor/mod", geracao: 9, instancia: 102, parou: false });
  await volta();
  await volta();

  confere(caso, vistas.length === 1, `colheu ${vistas.length} mensagem(ns)`);
  const pedido = b.chamadas.find((c) => c.cmd === "mod_nativo_colher");
  confere(caso, Boolean(pedido), "a janela não chamou a colheita");
  confere(
    caso,
    pedido?.args?.instancia === 102,
    "a colheita não nomeou a instância",
  );
  // E o aviso não traz carga: nada no payload além da identidade.
  confere(
    caso,
    !b.chamadas.some((c) => JSON.stringify(c.args ?? {}).includes("corpo")),
    "o aviso voltou a carregar o corpo da mensagem",
  );
}

(async () => {
  const provas = [
    falaAntigaNaoEntraDuranteASubida,
    confirmacaoTardiaConclui,
    umaSegundaTentativaDeEncerrarValeDeNovo,
    aJanelaColheEmVezDeReceber,
  ];
  for (const prova of provas) {
    try {
      await prova();
    } catch (erro) {
      falhas.push(`${prova.name}: lançou ${erro}`);
    }
  }
  if (falhas.length === 0) {
    // Contado, e não escrito: a frase dizia «três» com quatro provas na lista
    // desde que a quarta entrou, e um relatório que não confere com o que
    // rodou é exatamente o defeito que este repositório mais paga.
    console.log(`ciclo do executor: as ${provas.length} corridas passam`);
    process.exit(0);
  }
  for (const falha of falhas) console.error(`FALHOU — ${falha}`);
  process.exit(1);
})();
