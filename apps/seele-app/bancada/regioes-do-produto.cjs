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
  {
    ponto: "operador.barra",
    conteiner: "#operador-mods",
    // O núcleo (§10): o microfone, o fone, o «no ar» e o SAIR.
    doProduto: ["#botao-mudo", "#botao-surdo", "#voz-estado", "#operador-sair"],
    // Conteúdo maior que a região rola por dentro, e o núcleo continua inteiro
    // na janela.
    teto: { aVista: ["#botao-mudo", "#operador-sair"] },
  },
  {
    ponto: "compositor",
    conteiner: "#compositor-mods",
    // A caixa de escrever, o anexo e o ENVIAR: quem envia é a pessoa.
    doProduto: ["#campo-mensagem", "#botao-anexar", "#botao-enviar"],
    teto: { aVista: ["#campo-mensagem", "#botao-enviar"] },
  },
  {
    ponto: "status.barra",
    conteiner: "#telemetria-mods",
    // Atraso, jitter, distúrbio e codec: o que a pessoa lê para saber se a voz
    // está chegando.
    doProduto: ["#tel-rtt", "#tel-jit", "#tel-loss", "#tel-codec"],
    // E o aviso de falha local, que é do produto e fica depois da região:
    // escondido no retrato da bancada, ele é mostrado para a medida. Sem o
    // `overflow: hidden` da região, ele saía da janela (1 302–1 387 px numa
    // janela de 1 280, medido).
    teto: { aVista: ["#tel-rtt", "#tel-codec", "#tel-local"], mostrar: ["#tel-local"] },
  },
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
      // Uma região que lança não esconde as outras: a falha é dela, e a rodada
      // segue.
      const medido = await page.evaluate(async ({ ponto, conteiner, doProduto, teto }) => {
        const r = window.testTable.snapshot;
        const quadro = () => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
        const doProdutoAgora = () => doProduto.map((seletor) => document.querySelector(seletor)?.outerHTML ?? null);
        // O tamanho, e não só a marcação: um MOD largo espremeria as métricas
        // da barra de status sem mudar um caractere delas.
        const tamanhos = () => doProduto.map((seletor) => {
          const caixa = document.querySelector(seletor)?.getBoundingClientRect();
          return caixa ? `${Math.round(caixa.width)}×${Math.round(caixa.height)}` : null;
        });
        const tamanhosAntes = tamanhos();
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
        const tamanhosComOMod = tamanhos();

        contribuicoesDosMods.revogar(handle, { id: "mod/regiao", instancia });
        modsCarregados.delete("mod/regiao");
        desenhar(r);
        await quadro();
        const depois = { conteiner: document.querySelector(conteiner)?.innerHTML ?? null, produto: doProdutoAgora() };

        // **O teto**: 16 linhas longas, ~1 700 px de conteúdo — mais do que a
        // janela de 860 px, e dentro dos 64 nós de um cartão. Medido: com o
        // teto, a região mostra 96 px e o SAIR fica a 778 px do topo; sem ele,
        // o SAIR vai a 1 906 px, fora da janela. Muitas linhas curtas não
        // serviam: 60 passavam do orçamento e não montavam nada, e 30 (480 px)
        // cabiam na janela, encolhendo a lista de canais acima.
        let foraDaVista = null;
        let tamanhosNoTeto = null;
        if (teto) {
          const grande = contribuicoesDosMods.registrar({ id: "mod/regiao" }, instancia, {
            ponto,
            modo: "adicionar",
            conteudo: [
              ...Array.from({ length: 16 }, (_, i) => ({ forma: "texto", chave: `l${i}`, dentro: `linha ${i} `.repeat(24) })),
              // E uma linha sem quebra, ~2 500 px: é ela que espremeria uma
              // barra de altura fixa na horizontal.
              { forma: "texto", chave: "largo", dentro: "LARGO".repeat(80) },
            ],
          });
          modsCarregados.set("mod/regiao", instancia);
          desenhar(r);
          await quadro();
          tamanhosNoTeto = tamanhos();
          for (const seletor of teto.mostrar ?? []) document.querySelector(seletor).hidden = false;
          foraDaVista = teto.aVista.filter((seletor) => {
            const caixaDoNucleo = document.querySelector(seletor)?.getBoundingClientRect();
            return !caixaDoNucleo || caixaDoNucleo.height === 0
              || caixaDoNucleo.top < 0 || caixaDoNucleo.bottom > innerHeight
              || caixaDoNucleo.left < 0 || caixaDoNucleo.right > innerWidth;
          });
          for (const seletor of teto.mostrar ?? []) document.querySelector(seletor).hidden = true;
          contribuicoesDosMods.revogar(grande.handle, { id: "mod/regiao", instancia });
          modsCarregados.delete("mod/regiao");
          desenhar(r);
          await quadro();
        }
        return {
          existe: Boolean(caixa), antes, dentro, emOutroLugar, comOMod, depois, foraDaVista,
          tamanhosAntes, tamanhosComOMod, tamanhosNoTeto,
        };
      }, regiao).catch((erro) => ({ erro: erro.message.split("\n")[0] }));

      const { ponto } = regiao;
      if (medido.erro) {
        falhas.push(`${ponto}: ${medido.erro}`);
        continue;
      }
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
        } else if (medido.tamanhosComOMod[i] !== medido.tamanhosAntes[i]) {
          falhas.push(`${ponto}: ${seletor}, do produto, mudou de tamanho com o MOD de pé `
            + `(${medido.tamanhosAntes[i]} → ${medido.tamanhosComOMod[i]})`);
        }
      });
      if (medido.depois.conteiner !== medido.antes.conteiner) {
        falhas.push(`${ponto}: revogado, e ${regiao.conteiner} não voltou ao que era`);
      }
      if (medido.depois.produto.some((html, i) => html !== medido.antes.produto[i])) {
        falhas.push(`${ponto}: revogado, e o que é do produto não voltou ao que era`);
      }
      medido.tamanhosNoTeto?.forEach((tamanho, i) => {
        if (tamanho !== medido.tamanhosAntes[i]) {
          falhas.push(`${ponto}: com conteúdo grande, ${regiao.doProduto[i]}, do produto, mudou de tamanho `
            + `(${medido.tamanhosAntes[i]} → ${tamanho})`);
        }
      });
      if (medido.foraDaVista?.length) {
        const quais = medido.foraDaVista;
        falhas.push(`${ponto}: com conteúdo maior que a região, ${quais.join(" e ")} `
          + `${quais.length > 1 ? "saíram" : "saiu"} da vista`);
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
