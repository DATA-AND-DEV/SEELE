// **O ATUALIZAR da gestão de MODs diz o que aconteceu** (P1 da 0.17, achado do
// Codex no app nativo; combinado com o hosting-lifecycle). Com o HTML e os
// scripts reais; a ponte do Tauri é um duble.
//
// Sem servidor no ar, atualizar só traz o pacote a esta máquina: nenhum banco
// parado é tocado (o fan-out saiu), e a frase antiga — «instalado, e
// desligado» — afirmava um estado que ninguém mudou. Os três casos:
//
// 1. **reaplicado** (`true`): aplicado neste servidor, e quem está dentro
//    precisa aceitar de novo;
// 2. **hospedando, sem o MOD ligado aqui**: está nesta máquina, e este
//    servidor não o tem ligado;
// 3. **sem servidor no ar**: está nesta máquina e nenhum servidor mudou; um
//    servidor guardado continua com a versão que tinha.
//
// Rodar:  node apps/seele-app/bancada/atualizar-mod-sem-servidor.cjs

const assert = require("node:assert/strict");
const { servir, respostas } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const erros = [];
    page.on("pageerror", (e) => erros.push(e.message));
    await page.addInitScript((tabela) => {
      window.tabela = tabela;
      window.chamadas = [];
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => {
          chamadas.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
          return structuredClone(tabela[cmd] ?? null);
        } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({ mods_instalados: [], aceites_de_mods: [], pacotes_no_cache: [] }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);

    const atualizar = async ({ reaplicado, hospedando }) => {
      return page.evaluate(async ({ reaplicado, hospedando }) => {
        tabela.instalar_mod_do_catalogo = reaplicado;
        tabela.estou_hospedando = hospedando;
        chamadas.length = 0;
        await instalarDoCatalogo("seele/estilo", "4.0.0");
        return {
          frase: $("catalogo-estado").textContent,
          uso: chamadas.find((c) => c.cmd === "instalar_mod_do_catalogo")?.args?.uso,
        };
      }, { reaplicado, hospedando });
    };

    let r = await atualizar({ reaplicado: true, hospedando: true });
    assert.equal(r.uso, "atualizar", "a gestão deixou de pedir o uso «atualizar»");
    assert.match(r.frase, /instalado e aplicado neste servidor/, `reaplicado: ${r.frase}`);

    r = await atualizar({ reaplicado: false, hospedando: true });
    assert.match(r.frase, /Este servidor não o tem ligado/, `hospedando sem o MOD ligado: ${r.frase}`);

    r = await atualizar({ reaplicado: false, hospedando: false });
    assert.match(r.frase, /está nesta máquina, e nenhum servidor mudou/, `sem servidor no ar: ${r.frase}`);
    assert.match(r.frase, /continua com a versão que tinha/, `a frase não diz o que o servidor parado tem: ${r.frase}`);
    assert.doesNotMatch(r.frase, /desligado/, `sem servidor no ar, a frase afirma um estado que ninguém mudou: ${r.frase}`);

    assert.deepEqual(erros, [], `erros na página: ${erros.join(" | ")}`);
  } finally {
    await browser.close();
    server.close();
  }
  console.log("atualizar MOD sem servidor: a frase diz o que aconteceu — aplicado aqui, não ligado aqui, ou só nesta máquina, sem servidor nenhum mudado.");
})().catch((e) => { console.error(e); process.exit(1); });
