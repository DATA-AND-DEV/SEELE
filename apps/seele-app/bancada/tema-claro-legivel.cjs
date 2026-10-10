// O texto da sessão sob um tema claro, com o contraste **computado** pelo
// navegador, e não o que a validação do tema calcula.
//
// # Por que esta bancada existe
//
// Medido pela F8 e conferido pela F3 em 09/10/2026: com um tema claro que a
// própria validação aceita (`oTextoAlcancaOFundo()` devolve `true`), o corpo
// das mensagens do desktop saía em `rgb(234,227,207)` sobre `rgb(242,236,224)`,
// 1,09:1, e o SAIR DA SALA da barra do operador a 1,04:1. O `body` resolve
// `color: var(--seele-osso)` com o valor da raiz, o tema redefine o token em
// `#tela-sessao`, abaixo dele, e quem não declara cor herda a já resolvida. A
// validação passava e a tela não acompanhava — e o defeito estava na 0.15.1
// publicada, igual.
//
// # O que ela mede
//
// Três lugares que herdavam a cor resolvida acima de `#tela-sessao` — o corpo de
// uma mensagem, o SAIR DA SALA e o nome de uma pessoa no roster — e dois em cor
// de estado, que o produto deriva do tema (tarefa 17e do plano da F3): o número
// da sincronia, em fósforo, e o nome de quem usa, no azul. Cada um precisa de
// 4,5:1 contra o primeiro fundo opaco acima dele.
//
// **E o mesmo, por região** (achado da F8, 10/10; tarefa 17i, item 3, da F3).
// Um tema que pinta uma região clara escreve os tokens dela no contêiner do
// produto; quem está dentro e não declara cor herda a de `#tela-sessao`, já
// resolvida — medido pela F8 no `operador.barra`: um texto de contribuição a
// 1,09:1. Aqui, cada região de `REGIOES_DO_TEMA`, uma de cada vez, com um tema
// claro só nela: uma sonda de texto sem cor própria em cada contêiner, contra o
// primeiro fundo opaco acima dela.
//
// Rodar:  node apps/seele-app/bancada/tema-claro-legivel.cjs

const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(root, "apps/seele-app/bancada/telas.cjs"));
const chromium = require("./playwright.cjs").chromium();

/** Um tema claro completo, de seis cores, que a validação aceita. */
const TEMA_CLARO = {
  fundo: "#f2ece0",
  painel: "#e6dfd0",
  texto: "#12100e",
  apagado: "#5a5148",
  acento: "#8a4a00",
  borda: "#b8ad9c",
};

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    page.on("pageerror", (erro) => {
      throw erro;
    });
    await page.addInitScript(
      (table) => {
        window.testTable = table;
        window.__TAURI__ = {
          core: { invoke: async (cmd) => (Object.hasOwn(table, cmd) ? table[cmd] : null) },
          event: { listen: async () => () => {} },
          window: {
            getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }),
          },
        };
      },
      respostas({
        snapshot: null,
        microfones: [],
        saidas: [],
        pacotes_no_cache: [],
        aceites_de_mods: [],
        limite_da_mensagem: 4096,
        previas_consentidas: [],
        som_da_tela: { volume: 1, calada: false },
        exclusao_do_som_da_captura: "Excluido",
      }),
    );
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate((r) => {
      window.testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(r);
    }, retrato());
    await page.waitForSelector(".mensagens .corpo");

    const porRegiao = await page.evaluate((tema) => {
      const rgb = (s) => (s.match(/[\d.]+/g) || []).map(Number);
      const lum = ([r, g, b]) =>
        [r, g, b]
          .map((c) => {
            const v = c / 255;
            return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
          })
          .reduce((a, c, i) => a + c * [0.2126, 0.7152, 0.0722][i], 0);
      const razao = (a, b) => {
        const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
        return (x + 0.05) / (y + 0.05);
      };
      const fundoDe = (el) => {
        for (let n = el; n && n !== document.documentElement; n = n.parentElement) {
          const c = rgb(getComputedStyle(n).backgroundColor);
          if (c.length >= 3 && (c.length < 4 || c[3] > 0.5)) return c.slice(0, 3);
        }
        return rgb(getComputedStyle(document.body).backgroundColor).slice(0, 3);
      };
      const daRegiao = {
        fundo: tema.fundo,
        texto: tema.texto,
        apagado: tema.apagado,
        acento: tema.acento,
        borda: tema.borda,
      };
      // O fundo de cada contêiner sem tema nenhum, para o caso da região que
      // só muda o `texto`.
      const fundoProprio = (seletor) => {
        const no = document.querySelector(`#tela-sessao ${seletor}`);
        return no ? getComputedStyle(no).backgroundColor : null;
      };
      const semTema = Object.fromEntries(
        Object.values(REGIOES_DO_TEMA).flat().map((seletor) => [seletor, fundoProprio(seletor)]),
      );
      const saida = [];
      const soTexto = [];
      for (const [regiao, seletores] of Object.entries(REGIOES_DO_TEMA)) {
        // **Só o `texto`, claro**, que a validação aceita sobre o fundo da
        // sessão: nenhum contêiner troca de fundo.
        aplicarOTemaDoServidor({
          mod_id: "seele/estilo",
          valores: JSON.stringify({ regioes: { [regiao]: { texto: "#f4efe6" } } }),
        });
        try {
          for (const seletor of seletores) {
            soTexto.push({ regiao, seletor, antes: semTema[seletor], depois: fundoProprio(seletor) });
          }
        } finally {
          aplicarOTemaDoServidor(null);
        }
        aplicarOTemaDoServidor({
          mod_id: "seele/estilo",
          valores: JSON.stringify({ regioes: { [regiao]: daRegiao } }),
        });
        try {
          for (const seletor of seletores) {
            const conteiner = document.querySelector(`#tela-sessao ${seletor}`);
            if (!conteiner) {
              saida.push({ regiao, seletor, achado: false });
              continue;
            }
            const sonda = document.createElement("span");
            sonda.textContent = "texto de uma contribuição";
            conteiner.append(sonda);
            const cor = rgb(getComputedStyle(sonda).color).slice(0, 3);
            const fundo = fundoDe(sonda);
            sonda.remove();
            saida.push({
              regiao,
              seletor,
              achado: true,
              cor: `rgb(${cor.join(",")})`,
              fundo: `rgb(${fundo.join(",")})`,
              razao: Number(razao(cor, fundo).toFixed(2)),
            });
          }
        } finally {
          aplicarOTemaDoServidor(null);
        }
      }
      // A pergunta da orquestradora (10/10): uma região só com `texto` escuro,
      // sem `fundo`, contra qual fundo a validação mede? Aplicada, ela deixaria
      // o texto escuro sobre o fundo escuro da sessão.
      aplicarOTemaDoServidor({
        mod_id: "seele/estilo",
        valores: JSON.stringify({ regioes: { "pessoas.lista": { texto: tema.texto } } }),
      });
      const escuroSemFundo = document.querySelector("#tela-sessao .painel-pessoas").style.getPropertyValue("--seele-osso");
      aplicarOTemaDoServidor(null);
      return { saida, soTexto, escuroSemFundo };
    }, TEMA_CLARO);
    const { saida: medidasPorRegiao, soTexto, escuroSemFundo } = porRegiao;
    const regioesMedidas = new Set(medidasPorRegiao.map((m) => m.regiao));
    assert.equal(regioesMedidas.size, 8, `as regiões medidas não são as oito: ${[...regioesMedidas]}`);
    const ilegiveis = medidasPorRegiao.filter((m) => !m.achado || m.razao < 4.5);
    assert.deepEqual(
      ilegiveis,
      [],
      "sob um tema claro só na região, um texto sem cor própria no contêiner dela não alcança 4,5:1 " +
        "(a cor foi resolvida acima do contêiner, onde a região não chega, ou o contêiner não pinta o fundo dela)",
    );
    // Com `fundo` declarado, cada contêiner pinta o fundo da região.
    const semOFundo = medidasPorRegiao.filter((m) => m.fundo !== "rgb(242,236,224)");
    assert.deepEqual(semOFundo, [], "um contêiner não pinta o fundo que a região declarou");
    // Com só o `texto`, nenhum contêiner troca de fundo.
    const trocaram = soTexto.filter((m) => m.antes !== m.depois);
    assert.deepEqual(trocaram, [], "uma região que só muda o texto trocou o fundo de um contêiner");
    console.log("texto escuro sem fundo, na região:", escuroSemFundo === "" ? "recusado" : `aplicado (${escuroSemFundo})`);

    const medido = await page.evaluate((tema) => {
      aplicarOTemaDoMod("seele/estilo", tema);
      const rgb = (s) => (s.match(/[\d.]+/g) || []).map(Number);
      const lum = ([r, g, b]) =>
        [r, g, b]
          .map((c) => {
            const v = c / 255;
            return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
          })
          .reduce((a, c, i) => a + c * [0.2126, 0.7152, 0.0722][i], 0);
      const razao = (a, b) => {
        const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
        return (x + 0.05) / (y + 0.05);
      };
      const fundoDe = (el) => {
        for (let n = el; n && n !== document.documentElement; n = n.parentElement) {
          const c = rgb(getComputedStyle(n).backgroundColor);
          if (c.length >= 3 && (c.length < 4 || c[3] > 0.5)) return c.slice(0, 3);
        }
        return rgb(getComputedStyle(document.body).backgroundColor).slice(0, 3);
      };
      const medir = (seletor) => {
        const el = document.querySelector(seletor);
        if (!el) return { seletor, achado: false };
        const cor = getComputedStyle(el).color;
        const fundo = fundoDe(el);
        return {
          seletor,
          achado: true,
          cor,
          fundo: `rgb(${fundo.join(",")})`,
          razao: Number(razao(rgb(cor).slice(0, 3), fundo).toFixed(2)),
        };
      };
      return {
        legivel: oTemaAlcancaOsPisos(),
        lugares: [
          medir(".mensagens .corpo"),
          medir("#operador-sair"),
          medir(".roster .pessoa-nome"),
          // As cores de estado, derivadas do tema (tarefa 17e): o número da
          // sincronia, em fósforo, e o nome de quem usa, no azul.
          medir(".pessoa-sync-valor"),
          medir("#operador-nome"),
        ],
      };
    }, TEMA_CLARO);

    assert.equal(medido.legivel, true, "a validação recusou o tema claro de teste");
    for (const lugar of medido.lugares) {
      assert.ok(lugar.achado, `«${lugar.seletor}» não está na sessão desenhada`);
      assert.ok(
        lugar.razao >= 4.5,
        `«${lugar.seletor}» sai a ${lugar.razao}:1 sob o tema claro (${lugar.cor} sobre ` +
          `${lugar.fundo}): a cor foi resolvida acima de #tela-sessao, onde o tema não chega`,
      );
    }
    console.log(
      "tema claro legível:",
      medido.lugares.map((l) => `${l.seletor} ${l.razao}:1`).join(" · "),
      "· por região:",
      medidasPorRegiao.map((m) => `${m.regiao} ${m.seletor} ${m.razao}:1`).join(" · "),
    );
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro.message ?? erro);
  process.exit(1);
});
