// A tela de MODs oferece a última versão que ESTE build entende.
//
// # Por que ela existe
//
// A escolha era `versoes[versoes.length - 1]` — a última da lista, sem olhar
// `api`. Isso funciona todo dia menos o seguinte a uma subida de API: o
// catálogo lista tudo o que já foi publicado, a última entrada passa a pedir
// uma API que o build instalado não tem, e quem ainda não atualizou vê a
// versão nova oferecida, aperta instalar e recebe uma recusa — **sem caminho
// de volta** para a versão que funciona para ele, publicada logo acima na
// mesma lista.
//
// O runbook da v0.12.0 registrou o defeito e escreveu o conserto na mesma
// página, e ele ficou por fazer até a subida para a API 5 cobrar de novo.
//
// # Os três casos, e o terceiro é o que se esquece
//
// Um build de API 4 diante de um catálogo com 3.1.1 (API 4) e 3.2.0 (API 5)
// tem de oferecer a 3.1.1. Um de API 5 tem de oferecer a 3.2.0. E **sem a
// lista** — quando a resposta do Rust não chegou — tem de cair no
// comportamento de antes, a última da lista: filtrar por uma lista vazia
// esconderia o catálogo inteiro, que é pior do que oferecer uma versão que o
// produto recusa com uma frase.
//
//   node apps/seele-app/bancada/escolha-de-versao-de-mod.cjs
const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");

const CATALOGO = [{
  id: "seele/perfis", autor: "seele", nome: "perfis", titulo: "PERFIS",
  resumo: "perfis", repo: "", oficial: true, nivel: "oficial", commit: "abc", notas: [],
  versoes: [
    { versao: "3.1.1", api: 4, publicado_em: 1, hash: "h1", alcanca: [], arquivos: [], nivel: "oficial", notas: [], commit: "c1" },
    { versao: "3.2.0", api: 5, publicado_em: 2, hash: "h2", alcanca: [], arquivos: [], nivel: "oficial", notas: [], commit: "c2" },
  ],
}];

(async () => {
  const chromium = require("./playwright.cjs").chromium();
  const s = servir(); await new Promise(ok => s.listen(0, "127.0.0.1", ok));
  const nav = await chromium.launch({ headless: true });
  const aba = await nav.newPage({ viewport: { width: 1280, height: 860 } });
  const erros = []; aba.on("pageerror", e => erros.push(String(e.message).slice(0, 160)));
  try {
    const r = retrato();
    // Este build aceita 4 e 3 — como a v0.14.1 — e o catálogo já traz uma de API 5.
    await aba.addInitScript((t) => {
      window.__TAURI__ = {
        core: { invoke: async (c) => Object.hasOwn(t, c) ? t[c] : null },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested: () => {}, isMaximized: async () => false }) },
      };
    }, respostas({ snapshot: r, apis_de_mod_aceitas: [4, 3] }));
    await aba.goto(`http://127.0.0.1:${s.address().port}/`);
    await aba.waitForTimeout(900);

    const escolhida = await aba.evaluate((c) => ultimaQueEsteBuildEntende(c[0].versoes)?.versao, CATALOGO);
    assert.equal(escolhida, "3.1.1",
      `o catálogo ofereceu ${escolhida}: um build de API 4 está oferecendo a versão de API 5, ` +
      "que ele recusa ao instalar — e sem caminho de volta para a que funciona");

    // E um build que entende a 5 pega a 5.
    const comCinco = await aba.evaluate((c) => {
      apisDeModAceitas = [5, 4, 3];
      return ultimaQueEsteBuildEntende(c[0].versoes)?.versao;
    }, CATALOGO);
    assert.equal(comCinco, "3.2.0", "um build de API 5 deixou de oferecer a versão de API 5");

    // Sem a lista, o comportamento de antes: a última. Melhor que esconder tudo.
    const semLista = await aba.evaluate((c) => {
      apisDeModAceitas = [];
      return ultimaQueEsteBuildEntende(c[0].versoes)?.versao;
    }, CATALOGO);
    assert.equal(semLista, "3.2.0", "sem a lista de APIs a tela escondeu o catálogo em vez de cair no de antes");
  } finally { await nav.close(); s.close(); }
  assert.deepEqual([...new Set(erros)], [], "a página escreveu erro");
  console.log("escolha de versão: API 4 pega 3.1.1, API 5 pega 3.2.0, sem lista cai no de antes");
})().catch(e => { console.error(e); process.exitCode = 1; });
