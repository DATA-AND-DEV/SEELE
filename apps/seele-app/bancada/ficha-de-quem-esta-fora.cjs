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
//   6. a sessão que sai fecha a ficha.
//
//   PLAYWRIGHT=<playwright-core> node apps/seele-app/bancada/ficha-de-quem-esta-fora.cjs

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

    // ---- 6. a sessão sai: a ficha fecha.
    await page.evaluate(() => esquecerOEstadoDaSessao());
    assert.equal((await page.evaluate(ficha)).aberta, false, "a ficha seguiu aberta depois de a sessão sair");
    console.log("6. a sessão saiu: a ficha fechou");

    assert.deepEqual(erros, [], `a página lançou: ${erros.join(" | ")}`);
    console.log("ficha de quem está fora: ok");
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro);
  process.exit(1);
});
