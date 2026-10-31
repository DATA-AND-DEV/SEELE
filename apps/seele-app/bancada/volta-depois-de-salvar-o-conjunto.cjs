// **Quem hospeda não cai do próprio servidor ao salvar o conjunto de MODs**
// (e2e de 31/10, passo 1; `voltarSozinhoSeOConjuntoEMeu`, em `tela-fim.js`).
//
// A casca de verdade no navegador, com a ponte do Tauri simulada. A sessão
// acaba com `ModsMudaram`, como a de quem hospeda acaba no SALVAR:
//
//  1. o sim guardado para o endereço é o conjunto que o servidor desta janela
//     exige agora (o que o SALVAR gravou): a tela diz que o conjunto passou a
//     valer e volta sozinha — `disconnect` e então `connect` ao mesmo alvo;
//  2. o sim guardado é de outro conjunto (um membro, ou o conjunto mudou de
//     novo): a tela de sempre fica, com «OS MODS DESTE SERVIDOR MUDARAM» e
//     RECONECTAR, e nada se conecta sozinho;
//  3. a janela não hospeda (`conjunto_exigido_agora` vazio): idem.
//
//   PLAYWRIGHT=/caminho/para/node_modules/playwright node apps/seele-app/bancada/volta-depois-de-salvar-o-conjunto.cjs

const assert = require("node:assert/strict");
const { servir, respostas } = require("./telas.cjs");
const engines = require("./playwright.cjs");

const falhas = [];
async function caso(motor, numero, prova) {
  try {
    await prova();
  } catch (erro) {
    falhas.push(`${motor} · caso ${numero}: ${String(erro.message).split("\n")[0]}`);
  }
}

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
          window.ouvintes = [];
          window.__TAURI__ = {
            core: { invoke: async (cmd, args) => {
              chamadas.push({ cmd, args });
              const r = table[cmd];
              return typeof r === "function" ? r(args) : (r ?? null);
            } },
            event: { listen: async (nome, fn) => { ouvintes.push({ nome, fn }); return () => {}; } },
            window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
          };
        }, respostas({
          snapshot: null, amigos: [], contatos_da_agenda: [], microfones: [], saidas: [],
          pacotes_no_cache: [], aceites_de_mods: [], plataforma: "macos", maquinas_aceitas: [],
        }));
        await pagina.goto(`http://127.0.0.1:${servidor.address().port}`);
        await pagina.waitForFunction(() => typeof mostrarFim === "function" && typeof conectar === "function");

        /** A sessão de `alvo` acaba com `ModsMudaram`; a ponte responde o sim guardado e o exigido. */
        const acabar = (aceito, exigido) => pagina.evaluate(([a, e]) => {
          chamadas.length = 0;
          table.aceite_de_mods = () => a;
          table.conjunto_exigido_agora = () => e;
          // A volta para no `connect`: o que importa é que ela sai, e para onde.
          table.connect = () => new Promise(() => {});
          alvoDoServer = "127.0.0.1:8383";
          $("tela-sessao").hidden = false;
          $("tela-fim").hidden = true;
          mostrarFim("ModsMudaram");
        }, [aceito, exigido]);
        const chamou = (cmd) => pagina.evaluate((c) => chamadas.filter((x) => x.cmd === c).map((x) => x.args), cmd);

        // 1. O sim guardado é o conjunto exigido agora: volta sozinha.
        await caso(motor, "1", async () => {
          await acabar("conj-novo", "conj-novo");
          await pagina.waitForFunction(() => chamadas.some((c) => c.cmd === "connect"), null, { timeout: 5000 });
          assert.deepEqual(await chamou("aceite_de_mods"), [{ alvo: "127.0.0.1:8383" }], `${motor}: 1 · aceite_de_mods`);
          assert.equal((await chamou("disconnect")).length, 1, `${motor}: 1 · a sessão velha não foi desmontada antes de voltar`);
          const destino = JSON.stringify(await chamou("connect"));
          assert.match(destino, /127\.0\.0\.1:8383/, `${motor}: 1 · a volta não foi ao mesmo alvo: ${destino}`);
          const ordem = await pagina.evaluate(() => chamadas.map((c) => c.cmd));
          assert.ok(ordem.indexOf("disconnect") < ordem.indexOf("connect"), `${motor}: 1 · ordem ${ordem}`);
        });

        // 2. O sim guardado é de outro conjunto: a tela de sempre, sem volta.
        await caso(motor, "2", async () => {
          await acabar("conj-velho", "conj-novo");
          await pagina.waitForFunction(() => chamadas.some((c) => c.cmd === "conjunto_exigido_agora"), null, { timeout: 5000 });
          await pagina.waitForTimeout(300);
          assert.equal((await chamou("connect")).length, 0, `${motor}: 2 · conectou sozinho com o sim de outro conjunto`);
          const texto = await pagina.evaluate(() => $("fim-motivo").textContent);
          assert.match(texto, /OS MODS DESTE SERVIDOR MUDARAM/, `${motor}: 2 · ${texto}`);
          assert.match(await pagina.evaluate(() => $("botao-voltar").textContent), /RECONECTAR/, `${motor}: 2 · sem RECONECTAR`);
        });

        // 3. A janela não hospeda: idem.
        await caso(motor, "3", async () => {
          await acabar("conj-novo", "");
          await pagina.waitForFunction(() => chamadas.some((c) => c.cmd === "conjunto_exigido_agora"), null, { timeout: 5000 });
          await pagina.waitForTimeout(300);
          assert.equal((await chamou("connect")).length, 0, `${motor}: 3 · conectou sozinho sem hospedar`);
        });

        if (erros.length) falhas.push(`${motor}: erros na página: ${erros.join(" | ")}`);
      } finally {
        await navegador.close();
      }
    }
  } finally {
    servidor.close();
  }
  if (falhas.length) {
    console.error(falhas.join("\n"));
    process.exit(1);
  }
  console.log("volta depois de salvar o conjunto: quem hospeda volta sozinho com o próprio sim; os outros casos ficam na tela de sempre.");
})();
