// **A ficha de quem está fora do ar fica aberta.**
//
// O defeito (relatado pelo dono na 1.0.0, 13/11/2026): num servidor com uma
// pessoa conectada e catorze membros fora do ar, a ficha de qualquer um deles
// abria e fechava sozinha em meio segundo. `conferirAFichaAberta` fechava a
// ficha de quem não estivesse em `pessoasDaLista`, e esse mapa só recebe quem
// está CONECTADO (`linhaDoRoster`): o membro fora do ar e o perfil aberto pela
// página de perfis nunca entram nele, e o redesenho seguinte da lista fechava a
// ficha.
//
// O que esta bancada prende, na página de verdade com a ponte simulada:
//
//   1. a ficha aberta pela página de perfis, de um membro fora do ar, atravessa
//      os redesenhos, mostra o perfil dele e diz FORA DO AR (e não EM ESCUTA);
//   2. a de um membro fora do ar sem perfil, também;
//   3. a de quem só tem perfil conhecido (nem conectado, nem membro), também;
//   4. ela fecha sozinha quando a pessoa deixa de existir neste servidor: sai da
//      lista de membros, ou o perfil some, sem estar conectada;
//   5. quem está conectado e cai continua com a ficha aberta, que passa a dizer
//      FORA DO AR e perde o volume;
//   6. a linha de um membro FORA DO AR abre a ficha dele ao clique (aprovado
//      pelo dono em 13/11/2026): um botão de verdade, com o nome da pessoa no
//      nome acessível, do tamanho da linha — e a linha segue sendo só o nome,
//      sem horário, sem estado e sem ação de MOD;
//   7. pelo teclado também: o Tab chega à linha seguinte, e o Enter abre;
//   8. a sessão que sai fecha a ficha;
//   9. no celular, o toque na linha abre a folha da pessoa, com alvo de 44 px
//      e o nome no recuo das linhas de quem está aqui.
//
//   PLAYWRIGHT=<playwright-core> node apps/seele-app/bancada/ficha-de-quem-esta-fora.cjs
//
// Com `SEELE_PREVIEW_DIR=<pasta>`, ela deixa lá as capturas da linha com o foco
// e da lista e da folha no celular.

const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(root, "apps/seele-app/bancada/telas.cjs"));
const chromium = require("./playwright.cjs").chromium();

/** Catorze membros fora do ar, como no relato: ids 3 a 16. */
const NOMES = ["Marina", "Bruno", "Carla", "Davi", "Elisa", "Fabio", "Gil", "Helena", "Igor", "Joana", "Kleber", "Luna", "Mauro", "Nina"];
const membros = () => NOMES.map((nome, i) => ({ id: 3 + i, nickname: `${nome}#A${String(100 + i)}` }));

const perfil = (pessoa, nome, extra = {}) => ({
  pessoa, nome_exibido: nome, pronomes: "", status: "", bio: `bio de ${nome}`, cor: "#f2521f",
  efeito: "Nenhum", foto: null, capa: null, revisao: 1, ...extra,
});

/** O retrato do relato: uma pessoa conectada (quem usa) e, à parte, a Lia numa sala. */
function cena() {
  const base = retrato();
  const eu = base.presentes[0];
  const lia = base.presentes[1];
  return {
    ...base,
    voice_rooms: [{ ...base.voice_rooms[0], people: [eu, lia] }],
    presentes: [eu, lia],
    fora_do_ar_revision: 1,
    perfis_revision: 1,
    identidades_apresentadas: {},
  };
}

/** O que a ficha mostra agora. */
const ficha = () => {
  const camada = document.getElementById("ficha-pessoa");
  const corpo = document.getElementById("ficha-pessoa-corpo");
  return {
    aberta: !camada.hidden,
    titulo: document.getElementById("ficha-pessoa-titulo").textContent,
    cartaoDe: corpo.querySelector(".perfil-cartao")?.dataset.pessoa ?? null,
    bio: corpo.querySelector(".perfil-cartao-bio")?.textContent ?? "",
    estados: [...corpo.querySelectorAll(".ficha-estados .pastilha")].map((p) => p.textContent),
    volume: corpo.querySelectorAll("input.volume").length,
    moderar: corpo.querySelectorAll("button[data-moderar-pessoa]").length,
  };
};

/** A linha do FORA DO AR de um membro, como a página a desenhou. */
const linhaDoMembro = (id) => {
  const botao = document.querySelector(`#lista-roster li.membro-fora-do-ar button[data-ficha-pessoa="${id}"]`);
  const linha = botao?.closest("li.membro-fora-do-ar") ?? null;
  if (!linha) {
    return {
      tem: false,
      linhas: document.querySelectorAll("#lista-roster li.membro-fora-do-ar").length,
      botoesNasLinhas: document.querySelectorAll("#lista-roster li.membro-fora-do-ar button").length,
    };
  }
  const caixa = (no) => {
    const r = no.getBoundingClientRect();
    return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, height: r.height };
  };
  return {
    tem: true,
    texto: linha.textContent,
    botoes: linha.querySelectorAll("button").length,
    tipo: botao.type,
    nomeAcessivel: botao.getAttribute("aria-label") ?? botao.textContent,
    abre: botao.getAttribute("aria-haspopup"),
    noTeclado: botao.tabIndex,
    // O que a linha não pode ganhar junto com o botão: nada de MOD, nenhum
    // horário, nenhum estado, nenhum outro controle.
    alheios: [...linha.querySelectorAll("[data-acao-de-mod], [data-mod], a, input, select, time, .pastilha, .pessoa-estados")]
      .map((no) => no.className || no.localName),
    linha: caixa(linha),
    botao: caixa(botao),
    // As letras do nome dentro do alvo.
    letras: (() => {
      const faixa = document.createRange();
      faixa.selectNodeContents(botao);
      return caixa(faixa);
    })(),
    comOFoco: document.activeElement === botao,
  };
};

/** O que a ficha tem de MOD: ações e seções. */
const modsNaFicha = () =>
  document.querySelectorAll("#ficha-pessoa-corpo :is(.ficha-acoes-de-mod, .contribuicao-de-mod-caixa, [data-acao-de-mod])").length;

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1600, height: 917 } });
    page.setDefaultTimeout(10000);
    const erros = [];
    page.on("pageerror", (erro) => erros.push(String(erro)));
    await page.addInitScript(
      (table) => {
        window.testTable = table;
        window.__TAURI__ = {
          core: { invoke: async (cmd) => (Object.hasOwn(table, cmd) ? structuredClone(table[cmd]) : null) },
          event: { listen: async () => () => {} },
          window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
        };
      },
      respostas({
        snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [],
        membros_fora_do_ar: membros(),
        // A Marina (3) é membro fora do ar com perfil; a pessoa 40 só tem o
        // perfil: nem conectada, nem na lista de membros.
        perfis: [perfil(1, "Alex"), perfil(3, "Marina do Lago"), perfil(40, "Quem Saiu")],
      }),
    );
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.evaluate((r) => {
      window.testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(r);
    }, cena());

    /** O laço do produto: o retrato de agora, desenhado de novo, e um quadro para o que ele agendou. */
    const redesenhar = async (vezes = 3) => {
      for (let i = 0; i < vezes; i++) {
        await page.evaluate(() => {
          desenhar(structuredClone(window.testTable.snapshot));
          return new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
        });
      }
    };

    // A cena tem de ser a do relato, ou a bancada passaria por não ter montado nada.
    await page.waitForFunction(() => document.querySelectorAll("#lista-roster li.membro-fora-do-ar").length === 14);
    await page.waitForFunction(() => typeof perfilOficial === "function" && perfilOficial(3) !== null);
    await redesenhar(1);

    // ---- 1. pela página de perfis, um membro fora do ar com perfil.
    await page.evaluate(() => abrirDiretorioDosPerfis());
    await page.locator('#perfis-diretorio-corpo .perfil-cartao[data-pessoa="3"]').click();
    let visto = await page.evaluate(ficha);
    assert.ok(visto.aberta, `a ficha da Marina não abriu pela página de perfis: ${JSON.stringify(visto)}`);
    await redesenhar();
    visto = await page.evaluate(ficha);
    assert.ok(visto.aberta, `a ficha de um membro FORA DO AR fechou sozinha no redesenho seguinte da lista: ${JSON.stringify(visto)}`);
    assert.equal(visto.titulo, "Marina do Lago", `a ficha não é a da Marina: ${JSON.stringify(visto)}`);
    assert.equal(visto.cartaoDe, "3", `a ficha não mostra o perfil dela: ${JSON.stringify(visto)}`);
    assert.equal(visto.bio, "bio de Marina do Lago", `a ficha não mostra a bio do perfil: ${JSON.stringify(visto)}`);
    assert.deepEqual(visto.estados, ["FORA DO AR"], `a ficha de quem está fora do ar diz outro estado: ${JSON.stringify(visto)}`);
    assert.equal(visto.volume + visto.moderar, 0, `a ficha de quem está fora do ar oferece volume ou moderação: ${JSON.stringify(visto)}`);
    console.log(`1. membro fora do ar com perfil: aberta depois de 3 redesenhos, «${visto.titulo}», ${visto.estados.join("+")}`);
    await page.keyboard.press("Escape");
    assert.equal((await page.evaluate(ficha)).aberta, false, "Esc não fechou a ficha");

    // ---- 2. um membro fora do ar sem perfil: o apelido dele, e não o número.
    await page.evaluate(() => abrirPerfilDaPessoa("4"));
    await redesenhar();
    visto = await page.evaluate(ficha);
    assert.ok(visto.aberta, `a ficha de um membro FORA DO AR sem perfil fechou sozinha: ${JSON.stringify(visto)}`);
    assert.equal(visto.titulo, "Bruno#A101", `a ficha de quem está fora do ar não leva o apelido do membro no título: ${JSON.stringify(visto)}`);
    assert.deepEqual(visto.estados, ["FORA DO AR"], `a ficha diz outro estado: ${JSON.stringify(visto)}`);
    console.log(`2. membro fora do ar sem perfil: aberta, «${visto.titulo}»`);

    // ---- 4a. ele deixa de ser membro: a ficha fecha, e só depois de a lista nova chegar.
    await page.evaluate(() => {
      window.testTable.membros_fora_do_ar = window.testTable.membros_fora_do_ar.filter((m) => m.id !== 4);
      window.testTable.snapshot.fora_do_ar_revision = 2;
    });
    await redesenhar();
    await page.waitForFunction(() => document.querySelectorAll("#lista-roster li.membro-fora-do-ar").length === 13);
    await redesenhar(1);
    visto = await page.evaluate(ficha);
    assert.equal(visto.aberta, false, `a ficha de quem saiu do servidor seguiu aberta: ${JSON.stringify(visto)}`);
    console.log("4a. saiu da lista de membros: a ficha fechou");

    // ---- 3. só o perfil conhecido (nem conectada, nem membro).
    await page.evaluate(() => abrirPerfilDaPessoa("40"));
    await redesenhar();
    visto = await page.evaluate(ficha);
    assert.ok(visto.aberta, `a ficha de quem só tem perfil conhecido fechou sozinha: ${JSON.stringify(visto)}`);
    assert.equal(visto.cartaoDe, "40", `a ficha não mostra o perfil: ${JSON.stringify(visto)}`);
    console.log(`3. só com perfil conhecido: aberta, «${visto.titulo}»`);

    // ---- 4b. o perfil some: ninguém mais é essa pessoa aqui, e a ficha fecha.
    await page.evaluate(() => {
      window.testTable.perfis = window.testTable.perfis.filter((p) => p.pessoa !== 40);
      window.testTable.snapshot.perfis_revision = 2;
    });
    await redesenhar();
    await page.waitForFunction(() => perfilOficial(40) === null);
    await redesenhar(2);
    visto = await page.evaluate(ficha);
    assert.equal(visto.aberta, false, `a ficha de quem deixou de existir neste servidor seguiu aberta: ${JSON.stringify(visto)}`);
    console.log("4b. o perfil sumiu: a ficha fechou");

    // ---- 5. quem está conectado e cai: a ficha fica, e passa a dizer FORA DO AR.
    await page.locator('#lista-roster button[data-ficha-pessoa="2"]').click();
    visto = await page.evaluate(ficha);
    assert.ok(visto.aberta && visto.estados[0] !== "FORA DO AR" && visto.volume === 1,
      `a ficha de quem está na sua sala não abriu com o estado e o volume: ${JSON.stringify(visto)}`);
    await page.evaluate(() => {
      const r = window.testTable.snapshot;
      r.presentes = r.presentes.filter((p) => p.id !== 2);
      r.voice_rooms[0].people = r.voice_rooms[0].people.filter((p) => p.id !== 2);
      r.fora_do_ar_revision = 3;
      window.testTable.membros_fora_do_ar.push({ id: 2, nickname: "Lia" });
    });
    // O primeiro retrato sem ela chega antes de a lista de membros ser buscada:
    // por um quadro ela não está em lugar nenhum, e a ficha não pode fechar aí.
    await page.evaluate(() => desenhar(structuredClone(window.testTable.snapshot)));
    visto = await page.evaluate(ficha);
    assert.ok(visto.aberta, `a ficha fechou no quadro entre a saída e a lista de membros nova: ${JSON.stringify(visto)}`);
    await page.waitForFunction(() => document.querySelectorAll("#lista-roster li.membro-fora-do-ar").length === 14);
    await redesenhar();
    visto = await page.evaluate(ficha);
    assert.ok(visto.aberta, `a ficha de quem caiu fechou sozinha: ${JSON.stringify(visto)}`);
    assert.deepEqual(visto.estados, ["FORA DO AR"], `a ficha de quem caiu segue dizendo o estado de antes: ${JSON.stringify(visto)}`);
    assert.equal(visto.volume, 0, `a ficha de quem caiu segue com o volume: ${JSON.stringify(visto)}`);
    console.log(`5. conectada que caiu: aberta, ${visto.estados.join("+")}, sem volume`);

    await page.keyboard.press("Escape");

    // ---- 6. a linha do FORA DO AR abre a ficha, pelo clique.
    let linha = await page.evaluate(linhaDoMembro, 5);
    assert.ok(linha.tem, `a linha de um membro FORA DO AR não tem o botão que abre a ficha dele: ${JSON.stringify(linha)}`);
    assert.equal(linha.texto, "Carla#A102", `a linha do FORA DO AR diz mais que o nome: ${JSON.stringify(linha)}`);
    assert.deepEqual(linha.alheios, [], `a linha do FORA DO AR ganhou mais que o botão da ficha: ${JSON.stringify(linha)}`);
    assert.equal(linha.botoes, 1, `a linha do FORA DO AR tem mais de um botão: ${JSON.stringify(linha)}`);
    assert.ok(linha.tipo === "button" && linha.noTeclado === 0 && linha.abre === "dialog",
      `o alvo da linha não é um botão de verdade, no caminho do teclado, que anuncia o diálogo: ${JSON.stringify(linha)}`);
    assert.ok(linha.nomeAcessivel.includes("Carla#A102"), `o nome acessível do botão não diz de quem é a ficha: ${JSON.stringify(linha)}`);
    assert.ok(
      Math.abs(linha.botao.left - linha.linha.left) <= 1 && Math.abs(linha.botao.right - linha.linha.right) <= 1
        && Math.abs(linha.botao.top - linha.linha.top) <= 1 && Math.abs(linha.botao.bottom - linha.linha.bottom) <= 1,
      `o botão não ocupa a linha inteira: ${JSON.stringify(linha)}`,
    );
    assert.ok(linha.botao.height >= 24, `o alvo da linha tem menos de 24 px de altura: ${JSON.stringify(linha)}`);
    await page.locator('#lista-roster li.membro-fora-do-ar button[data-ficha-pessoa="5"]').click();
    await redesenhar();
    visto = await page.evaluate(ficha);
    assert.ok(visto.aberta, `o clique na linha de um membro FORA DO AR não abriu a ficha dele: ${JSON.stringify(visto)}`);
    assert.equal(visto.titulo, "Carla#A102", `a ficha aberta pela linha não é a do membro: ${JSON.stringify(visto)}`);
    assert.deepEqual(visto.estados, ["FORA DO AR"], `a ficha aberta pela linha diz outro estado: ${JSON.stringify(visto)}`);
    assert.equal(visto.volume + visto.moderar + await page.evaluate(modsNaFicha), 0,
      `a ficha aberta pela linha oferece volume, moderação ou ação de MOD: ${JSON.stringify(visto)}`);
    // Esc fecha, e o foco volta à linha: o nó dela atravessou os redesenhos.
    await page.keyboard.press("Escape");
    linha = await page.evaluate(linhaDoMembro, 5);
    assert.ok(!(await page.evaluate(ficha)).aberta && linha.comOFoco, `Esc não devolveu o foco à linha que abriu a ficha: ${JSON.stringify(linha)}`);
    console.log(`6. clique na linha do FORA DO AR: «${visto.titulo}», ${visto.estados.join("+")}, botão de ${Math.round(linha.botao.height)} px na linha inteira, foco de volta`);

    // ---- 7. e pelo teclado: o Tab chega à linha seguinte, o Enter abre.
    await page.keyboard.press("Tab");
    linha = await page.evaluate(linhaDoMembro, 6);
    assert.ok(linha.tem && linha.comOFoco, `o Tab não chegou à linha seguinte do FORA DO AR: ${JSON.stringify(linha)}`);
    if (process.env.SEELE_PREVIEW_DIR) await page.screenshot({ path: path.join(process.env.SEELE_PREVIEW_DIR, "fora-do-ar-com-o-foco-1600x917.png") });
    await page.keyboard.press("Enter");
    visto = await page.evaluate(ficha);
    assert.ok(visto.aberta && visto.titulo === "Davi#A103", `o Enter na linha de um membro FORA DO AR não abriu a ficha dele: ${JSON.stringify(visto)}`);
    console.log(`7. Tab e Enter na linha seguinte: «${visto.titulo}»`);

    // ---- 8. a sessão sai: a ficha fecha.
    await page.evaluate(() => esquecerOEstadoDaSessao());
    assert.equal((await page.evaluate(ficha)).aberta, false, "a ficha seguiu aberta depois de a sessão sair");
    console.log("8. a sessão saiu: a ficha fechou");

    assert.deepEqual(erros, [], `a página lançou: ${erros.join(" | ")}`);
    await page.close();

    // ---- 9. no celular: o toque na linha abre a folha da pessoa.
    const contexto = await browser.newContext({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true, deviceScaleFactor: 1 });
    const celular = await contexto.newPage();
    celular.setDefaultTimeout(10000);
    const errosDoCelular = [];
    celular.on("pageerror", (erro) => errosDoCelular.push(String(erro)));
    await celular.addInitScript(
      (table) => {
        new MutationObserver((_, o) => {
          if (!document.documentElement) return;
          document.documentElement.dataset.casca = "celular";
          o.disconnect();
        }).observe(document, { childList: true });
        window.testTable = table;
        window.__TAURI__ = {
          core: { invoke: async (cmd) => (Object.hasOwn(table, cmd) ? structuredClone(table[cmd]) : null) },
          event: { listen: async () => () => {} },
          window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
        };
      },
      respostas({
        snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [],
        membros_fora_do_ar: membros(),
        perfis: [perfil(1, "Alex"), perfil(3, "Marina do Lago")],
      }),
    );
    await celular.goto(`http://127.0.0.1:${server.address().port}/`, { waitUntil: "domcontentloaded" });
    await celular.waitForTimeout(700);
    await celular.evaluate((r) => {
      window.testTable.snapshot = r;
      for (const secao of document.querySelectorAll("section.tela")) secao.hidden = true;
      $("tela-sessao").hidden = false;
      $("tela-sessao").dataset.aba = "pessoas";
      desenhar(r);
    }, cena());
    assert.equal(await celular.evaluate(() => document.documentElement.dataset.casca), "celular", "a casca do celular não subiu");
    await celular.waitForFunction(() => document.querySelectorAll("#lista-roster li.membro-fora-do-ar").length === 14);
    await celular.waitForFunction(() => typeof perfilOficial === "function" && perfilOficial(3) !== null);
    linha = await celular.evaluate(linhaDoMembro, 3);
    assert.ok(linha.tem, `no celular, a linha de um membro FORA DO AR não tem o botão que abre a ficha dele: ${JSON.stringify(linha)}`);
    assert.ok(linha.botao.height >= 44, `no celular, o alvo de toque da linha tem menos de 44 px: ${JSON.stringify(linha)}`);
    assert.ok(Math.abs(linha.letras.left - linha.botao.left - 14) <= 1,
      `no celular, o nome do membro não tem o recuo de 14 px das linhas de quem está aqui (tem ${Math.round(linha.letras.left - linha.botao.left)}): ${JSON.stringify(linha)}`);
    const alvoNoCelular = celular.locator('#lista-roster li.membro-fora-do-ar button[data-ficha-pessoa="3"]');
    await alvoNoCelular.scrollIntoViewIfNeeded();
    if (process.env.SEELE_PREVIEW_DIR) await celular.screenshot({ path: path.join(process.env.SEELE_PREVIEW_DIR, "fora-do-ar-celular-390x844.png") });
    await alvoNoCelular.tap();
    await celular.waitForSelector("#cel-folha-pessoa", { state: "visible", timeout: 3000 }).catch(() => {});
    const folha = await celular.evaluate(() => {
      const no = document.querySelector("#cel-folha-pessoa");
      return {
        aberta: Boolean(no && no.getClientRects().length),
        titulo: no?.querySelector(".cel-folha-titulo")?.textContent ?? "",
        cartaoDe: no?.querySelector(".perfil-cartao")?.dataset.pessoa ?? null,
        estados: [...(no?.querySelectorAll(".ficha-estados .pastilha") ?? [])].map((p) => p.textContent),
        alheios: no?.querySelectorAll("input.volume, button[data-moderar-pessoa], .ficha-moderar-fora, .ficha-acoes-de-mod, .contribuicao-de-mod-caixa").length ?? 0,
      };
    });
    if (process.env.SEELE_PREVIEW_DIR) await celular.screenshot({ path: path.join(process.env.SEELE_PREVIEW_DIR, "fora-do-ar-folha-celular-390x844.png") });
    assert.ok(folha.aberta, `no celular, o toque na linha de um membro FORA DO AR não abriu a folha dele: ${JSON.stringify(folha)}`);
    assert.equal(folha.titulo, "Marina do Lago", `no celular, a folha não é a do membro: ${JSON.stringify(folha)}`);
    assert.equal(folha.cartaoDe, "3", `no celular, a folha não mostra o perfil dele: ${JSON.stringify(folha)}`);
    assert.deepEqual(folha.estados, ["FORA DO AR"], `no celular, a folha diz outro estado: ${JSON.stringify(folha)}`);
    assert.equal(folha.alheios, 0, `no celular, a folha de quem está fora do ar oferece volume, moderação ou MOD: ${JSON.stringify(folha)}`);
    assert.deepEqual(errosDoCelular, [], `a página do celular lançou: ${errosDoCelular.join(" | ")}`);
    console.log(`9. celular 390×844: toque na linha de ${Math.round(linha.botao.height)} px, folha «${folha.titulo}», ${folha.estados.join("+")}`);
    await contexto.close();
    console.log("ficha de quem está fora: ok");
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro);
  process.exit(1);
});
