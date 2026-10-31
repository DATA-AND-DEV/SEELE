// **O MOD que não roda aqui é pulado, e a casca diz por quê** (API 8, plataformas).
//
// A casca roda no navegador, com a ponte do Tauri simulada (`invoke("plataforma")`
// responde `"web"`, como `ponte-web.js`) e um catálogo falso respondido no lugar
// de `pedirAoServidor`, que é o que `carregarMods` consulta. Só o que está
// do outro lado da ponte é de mentira; `carregarMods`, `rodaAqui`, o aceite e a
// tela de MODs são os de verdade.
//
// O que se prova:
// 1. MOD `["app"]`: nenhum pedido de download, estado `so-noutra-plataforma`,
//    «só funciona no app» na tela de MODs, e ele continua EXIGIDO;
// 2. MOD sem `plataformas` (servidor 0.18): carregado como hoje;
// 3. MOD `["app", "navegador"]` e `["navegador"]`: carregados;
// 4. o aceite: com `plataformas` no anúncio (anfitrião 12) o de fora NÃO é baixado e o
//    `aceitar_mods` sai igual; sem `plataformas` (anfitrião 11) os quatro são baixados,
//    e o de fora, depois de instalado, não é montado;
// 5. a marca trocada pelo host depois de carregado descarrega o MOD.
//
//   PLAYWRIGHT=/caminho/para/node_modules/playwright node apps/seele-app/bancada/plataformas-dos-mods.cjs

const assert = require("node:assert/strict");
const { servir, respostas } = require("./telas.cjs");
const engines = require("./playwright.cjs");

const hash = (n) => `${n}`.padEnd(64, "0");
const CATALOGO = [
  { id: "x/so-app", version: "1.0.0", hash: hash("a"), plataformas: ["app"] },
  { id: "x/antigo", version: "1.0.0", hash: hash("b") },
  { id: "x/dois", version: "1.0.0", hash: hash("c"), plataformas: ["app", "navegador"] },
  { id: "x/so-web", version: "1.0.0", hash: hash("d"), plataformas: ["navegador"] },
];

(async () => {
  const servidor = servir();
  await new Promise((ok) => servidor.listen(0, "127.0.0.1", ok));
  try {
    for (const motor of ["chromium", "webkit"]) {
      const navegador = await engines[motor]().launch({ headless: true });
      try {
        const pagina = await navegador.newPage({ viewport: { width: 1400, height: 900 } });
        const erros = [];
        pagina.on("pageerror", (e) => erros.push(e.message));
        await pagina.addInitScript((tabela) => {
          window.table = tabela;
          window.chamadas = [];
          window.__TAURI__ = {
            core: { invoke: async (cmd, args) => {
              chamadas.push({ cmd, args });
              const r = table[cmd];
              return typeof r === "function" ? r(args) : (r ?? null);
            } },
            event: { listen: async () => () => {} },
            window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
          };
        }, respostas({
          snapshot: null, amigos: [], contatos_da_agenda: [], microfones: [], saidas: [],
          pacotes_no_cache: [], aceites_de_mods: [], plataforma: "web",
        }));
        await pagina.goto(`http://127.0.0.1:${servidor.address().port}`);
        await pagina.waitForFunction(() => !conferindoMods);

        // Os três que rodam aqui estão instalados; o de app, não.
        const r = await pagina.evaluate(async (catalogo) => {
          window.montados = [];
          table.mods_instalados = catalogo.filter((m) => m.id !== "x/so-app").map((m) => ({ id: m.id, hash: m.hash, client: true }));
          pedirAoServidor = async () => ({ ok: true, conjunto: "conj-1", mods: catalogo });
          montarOMod = async (mod) => montados.push(mod.id);
          entrarNaGeracao(1);
          chamadas.length = 0;
          await carregarMods();
          return {
            montados: [...montados].sort(),
            estado: estadoDosMods.get("x/so-app"),
            exigidos: [...modsExigidos.keys()].sort(),
            baixou: chamadas.filter((c) => c.cmd === "instalar_mod_do_catalogo").map((c) => c.args.id),
          };
        }, CATALOGO);
        assert.deepEqual(r.montados, ["x/antigo", "x/dois", "x/so-web"], `${motor}: carregou o que não devia ou deixou de carregar: ${JSON.stringify(r)}`);
        assert.equal(r.estado?.fase, "so-noutra-plataforma", `${motor}: ${JSON.stringify(r.estado)}`);
        assert.equal(r.estado.detalhe, "só funciona no app");
        assert.deepEqual(r.exigidos, CATALOGO.map((m) => m.id).sort(), `${motor}: o de fora deixou de ser exigido`);
        assert.deepEqual(r.baixou, [], `${motor}: baixou ${r.baixou}`);

        // A tela de MODs diz por quê, e continua dizendo que é exigido.
        await pagina.evaluate(async () => { await desenharMods(false); });
        const tela = await pagina.evaluate(() => $("lista-mods").innerText);
        assert.match(tela, /x\/so-app/, `${motor}: ${tela}`);
        assert.match(tela, /só funciona no app/, `${motor}: ${tela}`);
        assert.match(tela, /EXIGIDO POR ESTE SERVIDOR/, `${motor}: ${tela}`);
        assert.doesNotMatch(tela, /não está instalado aqui/, `${motor}: ${tela}`);

        // O aceite do conjunto, como o anúncio o entrega: itens `ModExigido`
        // (id, version, hash, repo, reach, no_servidor, tema) e, vindo de um
        // anfitrião 12, `plataformas`. Rodado duas vezes sobre o mesmo catálogo:
        // com a marca e sem ela.
        const aceitar = (comMarca) => pagina.evaluate(async ([catalogo, comMarca]) => {
          table.mods_instalados = [];
          table.instalar_mod_do_catalogo = ({ id }) => {
            const m = catalogo.find((c) => c.id === id);
            table.mods_instalados.push({ id, hash: m.hash, client: true });
          };
          chamadas.length = 0;
          const anunciados = catalogo.map(({ id, version, hash, plataformas }) => ({
            id, version, hash, repo: "r", reach: [], no_servidor: false, tema: false,
            ...(comMarca && plataformas ? { plataformas } : {}),
          }));
          await abrirAceiteDeMods("srv.exemplo:1", anunciados, "conj-1", null);
          await aceitarOsMods();
          return {
            baixou: chamadas.filter((c) => c.cmd === "instalar_mod_do_catalogo").map((c) => c.args.id).sort(),
            aceitou: chamadas.filter((c) => c.cmd === "aceitar_mods").map((c) => c.args),
          };
        }, [CATALOGO, comMarca]);

        // Anfitrião 12: o de fora não se baixa, e o aceite sai igual.
        const com = await aceitar(true);
        assert.deepEqual(com.aceitou, [{ alvo: "srv.exemplo:1", conjunto: "conj-1" }], `${motor}: ${JSON.stringify(com)}`);
        assert.deepEqual(com.baixou, ["x/antigo", "x/dois", "x/so-web"], `${motor}: baixou o que não roda aqui: ${JSON.stringify(com)}`);

        // Anfitrião 11: sem a marca nada sabe-se de fora, e baixam-se os quatro.
        const sem = await aceitar(false);
        assert.deepEqual(sem.aceitou, [{ alvo: "srv.exemplo:1", conjunto: "conj-1" }], `${motor}: ${JSON.stringify(sem)}`);
        assert.deepEqual(sem.baixou, CATALOGO.map((m) => m.id).sort(), `${motor}: ${JSON.stringify(sem)}`);

        // Já na sessão, o catálogo traz `plataformas`: o de app, agora INSTALADO,
        // não é montado e diz por quê.
        const depois = await pagina.evaluate(async (catalogo) => {
          modsCarregados.clear(); estadoDosMods.clear(); montados.length = 0;
          pedirAoServidor = async () => ({ ok: true, conjunto: "conj-1", mods: catalogo });
          entrarNaGeracao(3);
          await carregarMods();
          return { montados: [...montados].sort(), estado: estadoDosMods.get("x/so-app") };
        }, CATALOGO);
        assert.deepEqual(depois.montados, ["x/antigo", "x/dois", "x/so-web"], `${motor}: ${JSON.stringify(depois)}`);
        assert.equal(depois.estado?.fase, "so-noutra-plataforma", `${motor}: ${JSON.stringify(depois)}`);

        // O host trocou a marca de um MOD já carregado para fora daqui: descarrega.
        const trocou = await pagina.evaluate(async (catalogo) => {
          table.mods_instalados = catalogo.map((m) => ({ id: m.id, hash: m.hash, client: true }));
          modsCarregados.clear(); estadoDosMods.clear(); montados.length = 0;
          entrarNaGeracao(2);
          modsCarregados.set("x/dois", { encerrar: async () => {} });
          pedirAoServidor = async () => ({ ok: true, conjunto: "conj-2", mods: catalogo.map((m) => (m.id === "x/dois" ? { ...m, plataformas: ["app"] } : m)) });
          await carregarMods();
          return { carregado: modsCarregados.has("x/dois"), estado: estadoDosMods.get("x/dois"), montados: [...montados] };
        }, CATALOGO);
        assert.equal(trocou.carregado, false, `${motor}: continuou carregado`);
        assert.ok(!trocou.montados.includes("x/so-app"), `${motor}: montou o MOD só do app: ${JSON.stringify(trocou)}`);
        assert.equal(trocou.estado?.fase, "so-noutra-plataforma", `${motor}: ${JSON.stringify(trocou)}`);

        assert.deepEqual(erros, [], `${motor}: erros na página`);
        console.log(JSON.stringify({ motor, ok: true }));
        await pagina.close();
      } finally {
        await navegador.close();
      }
    }
  } finally {
    servidor.close();
  }
})().catch((e) => { console.error(e); process.exitCode = 1; });
