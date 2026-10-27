// O adaptador `invoke` de base.js: o desembrulho de `NaSessao`, as guardas que o
// precedem e o argumento de `ver_sessao`. Carrega a seção REAL de base.js num vm;
// só a fronteira IPC (o `invoke` do Tauri) é simulada. Sem navegador, sem Tauri.
// Uso: node bancada/envelope-do-invoke.cjs
const fs = require("node:fs");
const vm = require("node:vm");
const assert = require("node:assert/strict");
const path = require("node:path");

const fonte = fs.readFileSync(path.join(__dirname, "../ui/base.js"), "utf8");
const ini = fonte.indexOf("const invocarNaJanela =");
const fim = fonte.indexOf("/**\n * O `listen` da casca");
assert(ini > 0 && fim > ini, "seção do invoke não encontrada em base.js");
const trecho = fonte.slice(ini, fim);

/**
 * Um invoke cujas respostas o teste solta na ordem que quiser. O
 * `instancia_da_janela` (a instância desta carga, que o backend emite) é
 * respondido na hora, sem entrar nas `chamadas`: vem do `backend` dito (um que
 * sobrevive a recarregar), ou de um contador próprio; `semInstancia` faz o
 * comando não existir, como na ponte web. `agora` é o relógio desta carga.
 */
function montar({ backend = null, agora = 1e12, semInstancia = false } = {}) {
  const pendentes = [];
  const chamadas = [];
  let instanciasPedidas = 0;
  let propria = 0;
  const ctx = vm.createContext({
    Date: { now: () => agora },
    window: {
      __TAURI__: {
        core: {
          invoke: (comando, args) => {
            if (comando === "instancia_da_janela") {
              instanciasPedidas += 1;
              if (semInstancia) return Promise.reject("ComandoDesconhecido");
              propria += 1;
              return Promise.resolve(backend ? backend.instancia() : propria);
            }
            chamadas.push({ comando, args });
            return new Promise((resolve, reject) => pendentes.push({ comando, resolve, reject }));
          },
        },
        event: { listen() {} },
      },
    },
  });
  vm.runInContext(trecho, ctx);
  vm.runInContext("porNaVista({ sessao: 1, geracao: 2 }); epocaDaVista = 10;", ctx);
  const rodar = (js) => vm.runInContext(js, ctx);
  return {
    ctx,
    pendentes,
    chamadas,
    rodar,
    backend,
    get instancia() {
      return propria;
    },
    instanciasPedidas: () => instanciasPedidas,
  };
}

const motivo = (e) => e?.ForaDaVista?.motivo;
const carimbo = { sessao: 1, geracao: 2 };
let n = 0;
async function caso(nome, fn) {
  await fn();
  n += 1;
  console.log("ok", nome);
}

async function resolvido(resposta, antes) {
  const t = montar();
  const p = t.rodar('invoke("messages")');
  if (antes) t.rodar(antes);
  t.pendentes[0].resolve(resposta);
  return p.then((v) => ({ v }), (e) => ({ e }));
}

(async () => {
  const formas = {
    "array": ["a", { b: 1 }],
    "array vazio": [],
    "null": null,
    "objeto": { suas: [1], proprietario: true, gestao: { x: null } },
    "objeto com carimbo/valor dentro": { carimbo: { sessao: 9 }, valor: 1, extra: 2 },
    "recusa dentro do valor": { recusa: { Falta: "x" }, pessoas: [] },
    "zero": 0,
  };
  for (const [nome, valor] of Object.entries(formas)) {
    for (const [fmt, envelope] of [
      ["atual", { carimbo, epoca: 3, valor }],
      ["legado", { carimbo, valor }],
    ]) {
      await caso(`${fmt}: valor ${nome} chega sem envelope, intacto`, async () => {
        const { v, e } = await resolvido(envelope);
        assert.equal(e, undefined);
        assert.deepEqual(v, valor);
      });
    }
  }

  await caso("epoca do backend (1) difere do contador local (10) e a resposta vale", async () => {
    const { v } = await resolvido({ carimbo, epoca: 1, valor: ["x"] });
    assert.deepEqual(v, ["x"]);
  });

  await caso("snapshot achatado passa inteiro (carimbo+epoca+campos)", async () => {
    const snap = { carimbo, epoca: 3, rooms: [1], mic: 0 };
    const { v } = await resolvido(snap);
    assert.deepEqual(v, snap);
  });
  await caso("snapshot achatado com campo `valor` próprio não é desembrulhado", async () => {
    const snap = { carimbo, epoca: 3, valor: 7, rooms: [] };
    const { v } = await resolvido(snap);
    assert.deepEqual(v, snap);
  });
  await caso("resposta sem carimbo passa como veio", async () => {
    assert.deepEqual((await resolvido([1, 2])).v, [1, 2]);
    assert.deepEqual((await resolvido({ valor: 1 })).v, { valor: 1 });
    assert.equal((await resolvido(null)).v, null);
  });

  for (const [fmt, mk] of [
    ["atual", (c, valor) => ({ carimbo: c, epoca: 3, valor })],
    ["legado", (c, valor) => ({ carimbo: c, valor })],
  ]) {
    await caso(`${fmt}: outra sessão rejeitada, não desembrulhada`, async () => {
      const { v, e } = await resolvido(mk(carimbo, ["x"]), "porNaVista({sessao:2,geracao:2}); epocaDaVista++");
      assert.equal(v, undefined);
      assert.equal(motivo(e), "outra-sessao");
    });
    await caso(`${fmt}: retorno A→B→A é outra-visita`, async () => {
      const { e } = await resolvido(
        mk(carimbo, []),
        "porNaVista({sessao:2,geracao:2}); epocaDaVista++; porNaVista({sessao:1,geracao:2}); epocaDaVista++",
      );
      assert.equal(motivo(e), "outra-visita");
    });
    await caso(`${fmt}: geração velha rejeitada`, async () => {
      const { e } = await resolvido(mk({ sessao: 1, geracao: 1 }, []));
      assert.equal(motivo(e), "geracao-velha");
    });
    await caso(`${fmt}: resposta atrasada depois de sair da sessão (vista nula) é rejeitada`, async () => {
      const { v, e } = await resolvido(mk(carimbo, ["x"]), "vistaDaJanela.sessao = null");
      assert.equal(v, undefined);
      assert.equal(motivo(e), "outra-sessao");
    });
  }

  await caso("retry de SessaoNaoConfere: a resposta do refeito é desembrulhada", async () => {
    const t = montar();
    const p = t.rodar('invoke("messages")');
    t.pendentes[0].reject({ SessaoNaoConfere: { esperado: 5 } });
    await new Promise((r) => setImmediate(r));
    assert.equal(t.chamadas.length, 2);
    assert.equal(t.chamadas[1].args.carimbo.geracao, 5);
    t.pendentes[1].resolve({ carimbo: { sessao: 1, geracao: 5 }, epoca: 3, valor: [] });
    assert.deepEqual(await p, []);
  });
  await caso("retry: o refeito que volta com geração menor que a anotada é rejeitado", async () => {
    const t = montar();
    const p = t.rodar('invoke("messages")');
    t.pendentes[0].reject({ SessaoNaoConfere: { esperado: 5 } });
    await new Promise((r) => setImmediate(r));
    t.pendentes[1].resolve({ carimbo: { sessao: 1, geracao: 3 }, epoca: 3, valor: [] });
    await assert.rejects(p, (e) => motivo(e) === "geracao-velha");
  });
  await caso("resposta atrasada de um pedido, depois de uma troca local, é outra-visita", async () => {
    const { e } = await resolvido({ carimbo, epoca: 3, valor: ["velho"] }, "epocaDaVista++");
    assert.equal(motivo(e), "outra-visita");
  });

  // ---- ver_sessao ----
  // O `pedido` nativo é `{ instancia, n }`: a instância desta carga, emitida
  // pelo backend (`instancia_da_janela`), e um contador local. O backend de
  // mentira abaixo segue a regra de `sessoes::DominioDaVista` (só a instância
  // mais recente, e um `n` maior que o último), para provar o fluxo inteiro:
  // recarregar, inverter a ordem e o relógio recuando.
  const tique = () => new Promise((r) => setImmediate(r));
  await caso("ver_sessao envia `pedido: {instancia, n}` (nativo) e `epoca` (ponte web); n sobe a cada troca", async () => {
    const t = montar();
    const p1 = t.rodar('invoke("ver_sessao", { sessao: 2 })');
    const p2 = t.rodar('invoke("ver_sessao", { sessao: 3 })');
    await tique();
    const [a, b] = t.chamadas.map((c) => c.args);
    assert.equal(a.sessao, 2);
    assert.equal(JSON.stringify(a.pedido), JSON.stringify({ instancia: t.instancia, n: 1 }));
    assert.equal(JSON.stringify(b.pedido), JSON.stringify({ instancia: t.instancia, n: 2 }));
    assert.equal(typeof a.epoca, "number");
    assert.equal(b.epoca, a.epoca + 1);
    assert.equal(t.instanciasPedidas(), 1, "a instância foi pedida mais de uma vez na mesma carga");
    t.pendentes[0].reject("x");
    t.pendentes[1].reject("x");
    await Promise.allSettled([p1, p2]);
  });
  await caso("ver_sessao: o pedido velho que o nativo recusa vira VistaSuperada; a vista fica", async () => {
    const t = montar();
    const p1 = t.rodar('invoke("ver_sessao", { sessao: 2 })');
    const p2 = t.rodar('invoke("ver_sessao", { sessao: 3 })');
    await tique();
    t.pendentes[1].resolve({ carimbo: { sessao: 3, geracao: 1 }, geracao: 4 });
    t.pendentes[0].reject({ PedidoVelho: { pedido: { instancia: 1, n: 1 }, ultimo: { instancia: 1, n: 2 } } });
    assert.equal((await p2).carimbo.sessao, 3);
    await assert.rejects(p1, (e) => e?.VistaSuperada?.sessao === 2);
    assert.equal(t.rodar("vistaDaJanela.sessao"), 3);
  });
  await caso("ver_sessao: resposta bem-sucedida do pedido velho é descartada e não muda a vista", async () => {
    const t = montar();
    const p1 = t.rodar('invoke("ver_sessao", { sessao: 2 })');
    const p2 = t.rodar('invoke("ver_sessao", { sessao: 3 })');
    await tique();
    t.pendentes[1].resolve({ carimbo: { sessao: 3, geracao: 1 } });
    await p2;
    t.pendentes[0].resolve({ carimbo: { sessao: 2, geracao: 1 } });
    await assert.rejects(p1, (e) => !!e?.VistaSuperada);
    assert.equal(t.rodar("vistaDaJanela.sessao"), 3);
  });
  await caso("PedidoVelho em outro comando não é reescrito", async () => {
    const t = montar();
    const p = t.rodar('invoke("messages")');
    t.pendentes[0].reject({ PedidoVelho: { pedido: 1, ultimo: 2 } });
    await assert.rejects(p, (e) => !!e?.PedidoVelho);
  });

  /** O backend de mentira: a regra de `sessoes::DominioDaVista`, e a mesma instância por processo. */
  function backendDaVista() {
    let emitidas = 0;
    let emVigor = 0;
    let ultimo = 0;
    return {
      instancia() {
        emitidas += 1;
        emVigor = emitidas;
        ultimo = 0;
        return emitidas;
      },
      verSessao({ pedido }) {
        if (pedido.instancia !== emVigor || pedido.n <= ultimo) return { aceito: false };
        ultimo = pedido.n;
        return { aceito: true };
      },
    };
  }
  /** Uma carga da janela, com o relógio dito, sobre um backend que sobrevive a ela. */
  function carga(backend, agora) {
    const t = montar({ backend, agora });
    return t;
  }
  async function trocar(t, sessao) {
    const p = t.rodar(`invoke("ver_sessao", { sessao: ${sessao} })`);
    await tique();
    const chamada = t.chamadas[t.chamadas.length - 1];
    const { aceito } = t.backend.verSessao(chamada.args);
    const pendente = t.pendentes[t.pendentes.length - 1];
    if (aceito) pendente.resolve({ carimbo: { sessao, geracao: 1 } });
    else pendente.reject({ PedidoVelho: { pedido: chamada.args.pedido, ultimo: null } });
    return p.then(() => "aceito", (e) => (e?.VistaSuperada ? "recusado" : `erro ${JSON.stringify(e)}`));
  }

  await caso("recarregar: o primeiro pedido da página nova é aceito (o Date.now() + época falhava aqui)", async () => {
    const backend = backendDaVista();
    const antes = carga(backend, 100000);
    for (let i = 0; i < 101; i += 1) assert.equal(await trocar(antes, 2 + (i % 2)), "aceito");
    const depois = carga(backend, 100001);
    assert.equal(await trocar(depois, 2), "aceito", "depois de recarregar, o pedido válido foi recusado");
    assert.equal(await trocar(depois, 3), "aceito");
  });
  await caso("o relógio recuando não muda nada: o pedido não depende de relógio", async () => {
    const backend = backendDaVista();
    const antes = carga(backend, 100000);
    assert.equal(await trocar(antes, 2), "aceito");
    const depois = carga(backend, 99900);
    assert.equal(await trocar(depois, 3), "aceito", "com o relógio recuando, o pedido válido foi recusado");
    const a = depois.chamadas[depois.chamadas.length - 1].args.pedido;
    assert.equal(JSON.stringify(a).includes("999"), false, "o pedido carrega o relógio");
  });
  await caso("inverter a ordem: o pedido 1 chegando depois do 2 é recusado, e a vista fica no 2", async () => {
    const backend = backendDaVista();
    const t = carga(backend, 5);
    const p1 = t.rodar('invoke("ver_sessao", { sessao: 2 })');
    const p2 = t.rodar('invoke("ver_sessao", { sessao: 3 })');
    await tique();
    const [c1, c2] = t.chamadas.map((c) => c.args);
    assert.equal(backend.verSessao(c2).aceito, true);
    t.pendentes[1].resolve({ carimbo: { sessao: 3, geracao: 1 } });
    await p2;
    assert.equal(backend.verSessao(c1).aceito, false, "o pedido 1, atrasado, passou no backend");
    t.pendentes[0].reject({ PedidoVelho: { pedido: c1.pedido, ultimo: c2.pedido } });
    await assert.rejects(p1, (e) => !!e?.VistaSuperada);
    assert.equal(t.rodar("vistaDaJanela.sessao"), 3);
  });
  await caso("o pedido em voo da página de antes, chegando depois do reload, é recusado", async () => {
    const backend = backendDaVista();
    const antes = carga(backend, 1);
    const emVoo = antes.rodar('invoke("ver_sessao", { sessao: 2 })');
    await tique();
    const doAntes = antes.chamadas[0].args;
    const depois = carga(backend, 2);
    assert.equal(await trocar(depois, 3), "aceito");
    assert.equal(backend.verSessao(doAntes).aceito, false, "o pedido da página de antes passou depois dela");
    antes.pendentes[0].reject("descartado");
    await Promise.allSettled([emVoo]);
  });
  await caso("nova geração de uma sessão não mexe no domínio dos pedidos de vista", async () => {
    const backend = backendDaVista();
    const t = carga(backend, 7);
    assert.equal(await trocar(t, 2), "aceito");
    t.rodar("anotarGeracao({ sessao: 2, geracao: 9 })");
    assert.equal(await trocar(t, 3), "aceito");
    assert.deepEqual(t.chamadas[t.chamadas.length - 1].args.pedido.n, 2);
  });
  await caso("sem instância (a ponte web não a tem), o pedido não vai, e a época vai", async () => {
    const t = montar({ semInstancia: true });
    const p = t.rodar('invoke("ver_sessao", { sessao: 2 })');
    await tique();
    const args = t.chamadas[0].args;
    assert.equal(args.pedido, undefined);
    assert.equal(typeof args.epoca, "number");
    t.pendentes[0].reject("x");
    await Promise.allSettled([p]);
  });

  console.log(`\n${n} casos ok`);
})().catch((e) => {
  console.error("FALHOU:", e);
  process.exit(1);
});
