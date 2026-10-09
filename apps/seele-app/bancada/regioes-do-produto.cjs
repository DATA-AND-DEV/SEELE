// As regiões da API 6, no `index.html` real e no Chromium (rascunho da API 6,
// §3; tarefa 5 do plano da F3).
//
// # Por que esta bancada existe
//
// Os guardas de `frontend.rs` conferem que cada região tem nome, contêiner e
// alguém que pergunta por ela. Não conferem que o nó de um MOD aparece onde a
// região diz, nem que a apresentação nativa volta quando ele sai — e é isso
// que um MOD que registra e não vê nada encontraria. A bancada de
// contribuições roda num `vm`, sem navegador («não é navegador»); esta mede a
// página.
//
// # O que ela mede, por região
//
// 1. registrar `adicionar`: o nó do MOD aparece **dentro** do contêiner da
//    região;
// 2. o que a região deixa ao produto continua lá, intacto (o servidor atual e
//    «conectar a outro», na trilha; o SAIR e o microfone, na barra do
//    operador; …);
// 3. revogar: o nó sai, e a região volta a ser o que era antes, do estado
//    atual.
//
// Rodar:  node apps/seele-app/bancada/regioes-do-produto.cjs

const path = require("node:path");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(root, "apps/seele-app/bancada/telas.cjs"));
const chromium = require("./playwright.cjs").chromium();

/**
 * As regiões medidas: o ponto, o seletor do contêiner, e o que o produto
 * mantém nela — seletores que precisam continuar na página, do mesmo jeito,
 * com o MOD de pé.
 */
const REGIOES = [
  { ponto: "redes", conteiner: "#trilha-mods", doProduto: ["#trilha-server", "#trilha-adicionar"] },
];

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  const falhas = [];
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    page.on("pageerror", (erro) => falhas.push(`erro na página: ${erro.message}`));
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

    for (const regiao of REGIOES) {
      const medido = await page.evaluate(async ({ ponto, conteiner, doProduto }) => {
        const r = window.testTable.snapshot;
        const quadro = () => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
        const doProdutoAgora = () => doProduto.map((seletor) => document.querySelector(seletor)?.outerHTML ?? null);
        const caixa = document.querySelector(conteiner);
        const antes = { conteiner: caixa?.innerHTML ?? null, produto: doProdutoAgora() };

        // Uma instância viva, como a de um MOD carregado: sem ela a contribuição
        // não monta (`montarContribuicao` pede `instancia.admite`).
        const instancia = new InstanciaDeMod("mod/regiao", "h", geracaoDaSessao, {
          pedirEncerramento() {},
          async encerrou() { return true; },
        });
        instancia.estado = ESTADOS_DE_MOD.ativa;
        modsCarregados.set("mod/regiao", instancia);
        const { handle } = contribuicoesDosMods.registrar({ id: "mod/regiao" }, instancia, {
          ponto,
          modo: "adicionar",
          conteudo: [{ forma: "texto", chave: "marca", dentro: `marca de ${ponto}` }],
        });
        desenhar(r);
        await quadro();
        const dentro = [...(document.querySelector(conteiner)?.querySelectorAll("*") ?? [])]
          .some((no) => no.textContent === `marca de ${ponto}` && no.children.length === 0);
        const emOutroLugar = [...document.querySelectorAll("body *")]
          .some((no) => no.textContent === `marca de ${ponto}` && no.children.length === 0
            && !no.closest(conteiner));
        const comOMod = doProdutoAgora();

        contribuicoesDosMods.revogar(handle, { id: "mod/regiao", instancia });
        modsCarregados.delete("mod/regiao");
        desenhar(r);
        await quadro();
        const depois = { conteiner: document.querySelector(conteiner)?.innerHTML ?? null, produto: doProdutoAgora() };
        return { existe: Boolean(caixa), antes, dentro, emOutroLugar, comOMod, depois };
      }, regiao);

      const { ponto } = regiao;
      if (!medido.existe) {
        falhas.push(`${ponto}: o contêiner ${regiao.conteiner} não está na página`);
        continue;
      }
      if (!medido.dentro) falhas.push(`${ponto}: registrado, e o nó do MOD não apareceu em ${regiao.conteiner}`);
      if (medido.emOutroLugar) falhas.push(`${ponto}: o nó do MOD apareceu fora de ${regiao.conteiner}`);
      regiao.doProduto.forEach((seletor, i) => {
        if (medido.antes.produto[i] === null) falhas.push(`${ponto}: ${seletor}, do produto, não está na página`);
        else if (medido.comOMod[i] !== medido.antes.produto[i]) {
          falhas.push(`${ponto}: ${seletor}, do produto, mudou com o MOD de pé`);
        }
      });
      if (medido.depois.conteiner !== medido.antes.conteiner) {
        falhas.push(`${ponto}: revogado, e ${regiao.conteiner} não voltou ao que era`);
      }
      if (medido.depois.produto.some((html, i) => html !== medido.antes.produto[i])) {
        falhas.push(`${ponto}: revogado, e o que é do produto não voltou ao que era`);
      }
    }
  } finally {
    await browser.close();
    server.close();
  }
  if (falhas.length) {
    console.error(`regioes-do-produto: ${falhas.length} falha(s)`);
    for (const falha of falhas) console.error(`  ${falha}`);
    process.exit(1);
  }
  console.log(`regioes-do-produto: ok (${REGIOES.map((r) => r.ponto).join(", ")})`);
})().catch((erro) => {
  console.error(erro.message ?? erro);
  process.exit(1);
});
