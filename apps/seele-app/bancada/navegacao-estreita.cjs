// **Na janela estreita, as portas das gavetas moram numa barra da grade.**
//
// O defeito (relatado pelo dono na 1.0.0, 13/11/2026, com captura): abaixo de
// 1240 px a faixa de pessoas recolhe e ganha a porta «PESSOAS»; abaixo de 900,
// a coluna de salas ganha a «SALAS E CANAIS». As duas apareciam soltas no canto
// de cima e à esquerda, acima da trilha e dos painéis: pílulas de contorno fino
// e texto apagado direto sobre o fundo preto, sem superfície e sem relação com
// a grade — e a PESSOAS, do lado oposto ao da gaveta que ela abre.
//
// O que se quer, e o que esta bancada mede no DOM desenhado, em 900×1600,
// 760×560 e 1100×800:
//
//   · a navegação é uma barra com a superfície dos painéis: o mesmo fundo, a
//     mesma borda e o mesmo raio do painel da conversa;
//   · ela está na grade: começa na borda esquerda da trilha, acaba na borda
//     direita da telemetria (que, até 900 px, é a do painel da conversa), fica
//     a um vão de grade dos painéis e inteira dentro da janela;
//   · as portas têm a cor de um controle (texto `--seele-osso`, fundo e borda de
//     controle), cabem na barra e ficam centradas nela;
//   · cada porta fica do lado do que ela abre: SALAS E CANAIS encostada à
//     esquerda, PESSOAS encostada à direita, em cima da gaveta de pessoas;
//   · e continuam abrindo e fechando as gavetas.
//
//   PLAYWRIGHT=<playwright-core> node apps/seele-app/bancada/navegacao-estreita.cjs
//
// Com `SEELE_PREVIEW_DIR=<pasta>`, ela deixa lá uma captura por janela.

const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(root, "apps/seele-app/bancada/telas.cjs"));
const chromium = require("./playwright.cjs").chromium();

/** A barra, as portas à vista e os painéis contra os quais elas se alinham. */
const medir = () => {
  const caixa = (no) => {
    if (!no) return null;
    const r = no.getBoundingClientRect();
    return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, width: r.width, height: r.height };
  };
  const pele = (no) => {
    const e = getComputedStyle(no);
    return { fundo: e.backgroundColor, borda: `${e.borderTopWidth} ${e.borderTopStyle} ${e.borderTopColor}`, raio: e.borderTopLeftRadius };
  };
  const nav = document.querySelector(".sessao-navegacao");
  const conversa = document.querySelector(".paineis > .painel-linha");
  const estiloDaNav = getComputedStyle(nav);
  // O que um controle veste, lido dos tokens de agora.
  const sonda = document.createElement("span");
  sonda.style.cssText = "color: var(--seele-osso); background: var(--seele-controle); border: 1px solid var(--seele-controle-borda);";
  document.body.append(sonda);
  const controle = { cor: getComputedStyle(sonda).color, ...pele(sonda) };
  sonda.remove();

  // O contraste do texto de uma porta contra o que está embaixo dela: o fundo
  // da página, o da barra e o da porta, um por cima do outro.
  const rgba = (texto) => {
    const n = texto.match(/[\d.]+/g).map(Number);
    return { r: n[0], g: n[1], b: n[2], a: n.length > 3 ? n[3] : 1 };
  };
  const sobre = (cima, baixo) => ({
    r: cima.r * cima.a + baixo.r * (1 - cima.a), g: cima.g * cima.a + baixo.g * (1 - cima.a), b: cima.b * cima.a + baixo.b * (1 - cima.a), a: 1,
  });
  const luz = ({ r, g, b }) => {
    const canal = (v) => { const s = v / 255; return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4; };
    return 0.2126 * canal(r) + 0.7152 * canal(g) + 0.0722 * canal(b);
  };
  const contraste = (porta) => {
    let fundo = rgba(getComputedStyle(document.body).backgroundColor);
    if (fundo.a === 0) fundo = { r: 0, g: 0, b: 0, a: 1 };
    fundo = sobre(rgba(estiloDaNav.backgroundColor), fundo);
    fundo = sobre(rgba(getComputedStyle(porta).backgroundColor), fundo);
    const [clara, escura] = [luz(rgba(getComputedStyle(porta).color)), luz(fundo)].sort((a, b) => b - a);
    return Math.round(((clara + 0.05) / (escura + 0.05)) * 10) / 10;
  };

  const portas = [...nav.querySelectorAll("button")].filter((b) => b.getClientRects().length).map((porta) => ({
    classe: porta.className, texto: porta.textContent, caixa: caixa(porta),
    cor: getComputedStyle(porta).color, ...pele(porta), contraste: contraste(porta),
  }));
  return {
    janela: { largura: innerWidth, altura: innerHeight },
    nav: { visivel: nav.getClientRects().length > 0, caixa: caixa(nav), ...pele(nav),
      recuo: parseFloat(estiloDaNav.paddingLeft) + parseFloat(estiloDaNav.borderLeftWidth) },
    conversa: { caixa: caixa(conversa), ...pele(conversa) },
    trilha: caixa(document.querySelector(".paineis > .trilha")),
    telemetria: caixa(document.querySelector(".telemetria")),
    vaoDaGrade: parseFloat(getComputedStyle(document.querySelector("#tela-sessao .paineis")).columnGap),
    controle, portas,
  };
};

const dizer = (visto) => JSON.stringify(visto, (_, v) => (typeof v === "number" ? Math.round(v * 10) / 10 : v));

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  try {
    for (const [largura, altura, esperadas] of [
      [900, 1600, ["SALAS E CANAIS", "PESSOAS"]],
      [760, 560, ["SALAS E CANAIS", "PESSOAS"]],
      [1100, 800, ["PESSOAS"]],
    ]) {
      const page = await browser.newPage({ viewport: { width: largura, height: altura } });
      page.setDefaultTimeout(10000);
      const erros = [];
      page.on("pageerror", (erro) => erros.push(String(erro)));
      await page.addInitScript(
        (table) => {
          window.testTable = table;
          window.__TAURI__ = {
            core: { invoke: async (cmd) => (Object.hasOwn(table, cmd) ? table[cmd] : null) },
            event: { listen: async () => () => {} },
            window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
          };
        },
        respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }),
      );
      await page.goto(`http://127.0.0.1:${server.address().port}/`);
      await page.evaluate((r) => {
        window.testTable.snapshot = r;
        $("tela-boot").hidden = true;
        $("tela-sessao").hidden = false;
        desenhar(r);
      }, retrato());
      await page.evaluate(() => sincronizarMensagens(99));
      await page.waitForFunction(() => document.querySelectorAll("#lista-mensagens > li.mensagem").length >= 2);
      await page.evaluate(() => document.fonts.ready);

      const onde = `${largura}×${altura}`;
      if (process.env.SEELE_PREVIEW_DIR) {
        await page.screenshot({ path: path.join(process.env.SEELE_PREVIEW_DIR, `navegacao-${largura}x${altura}.png`) });
      }
      const visto = await page.evaluate(medir);
      const dito = dizer(visto);
      const { nav, conversa, trilha, telemetria, controle, portas } = visto;

      // A cena é a do relato: as portas desta largura, e só elas.
      assert.ok(nav.visivel, `${onde}: a navegação não apareceu: ${dito}`);
      assert.deepEqual(portas.map((p) => p.texto), esperadas, `${onde}: as portas à vista não são as desta largura: ${dito}`);

      // 1. A superfície dos painéis.
      assert.equal(nav.fundo, conversa.fundo, `${onde}: a navegação não tem o pano de fundo dos painéis (${nav.fundo}, e o painel da conversa tem ${conversa.fundo}): ${dito}`);
      assert.equal(nav.borda, conversa.borda, `${onde}: a navegação não tem a borda dos painéis (${nav.borda}, e o painel tem ${conversa.borda}): ${dito}`);
      assert.equal(nav.raio, conversa.raio, `${onde}: a navegação não tem o raio dos painéis (${nav.raio}, e o painel tem ${conversa.raio}): ${dito}`);

      // 2. Na grade, e dentro da janela.
      assert.ok(Math.abs(nav.caixa.left - trilha.left) <= 1, `${onde}: a navegação não começa na borda esquerda da trilha (${Math.round(nav.caixa.left)} e ${Math.round(trilha.left)}): ${dito}`);
      assert.ok(Math.abs(nav.caixa.right - telemetria.right) <= 1, `${onde}: a navegação não acaba na borda direita da telemetria (${Math.round(nav.caixa.right)} e ${Math.round(telemetria.right)}): ${dito}`);
      if (largura <= 900) {
        assert.ok(Math.abs(nav.caixa.right - conversa.caixa.right) <= 1, `${onde}: a navegação não acaba na borda direita do painel da conversa (${Math.round(nav.caixa.right)} e ${Math.round(conversa.caixa.right)}): ${dito}`);
      }
      assert.ok(Math.abs(conversa.caixa.top - nav.caixa.bottom - visto.vaoDaGrade) <= 1, `${onde}: a navegação não fica a um vão de grade (${visto.vaoDaGrade} px) dos painéis, e sim a ${Math.round(conversa.caixa.top - nav.caixa.bottom)}: ${dito}`);
      assert.ok(nav.caixa.left >= 0 && nav.caixa.top >= 0 && nav.caixa.right <= largura && nav.caixa.bottom <= altura, `${onde}: a navegação sai da janela: ${dito}`);

      // 3. As portas: a cor de um controle, dentro da barra, centradas nela.
      const meio = (c) => (c.top + c.bottom) / 2;
      for (const porta of portas) {
        const qual = `${onde}, «${porta.texto}»`;
        assert.equal(porta.cor, controle.cor, `${qual}: o texto da porta não tem a cor de um controle (${porta.cor}, e o controle tem ${controle.cor}; contraste ${porta.contraste}:1): ${dito}`);
        assert.equal(porta.fundo, controle.fundo, `${qual}: a porta não tem o fundo de um controle (${porta.fundo}): ${dito}`);
        assert.equal(porta.borda, controle.borda, `${qual}: a porta não tem a borda de um controle (${porta.borda}): ${dito}`);
        assert.ok(porta.caixa.height >= 40, `${qual}: a porta tem menos de 40 px de altura: ${dito}`);
        assert.ok(porta.caixa.left >= nav.caixa.left + nav.recuo - 1 && porta.caixa.right <= nav.caixa.right - nav.recuo + 1
          && porta.caixa.top >= nav.caixa.top && porta.caixa.bottom <= nav.caixa.bottom, `${qual}: a porta sai da barra: ${dito}`);
        assert.ok(Math.abs(meio(porta.caixa) - meio(nav.caixa)) <= 1, `${qual}: a porta não está centrada na altura da barra: ${dito}`);
        assert.ok(porta.caixa.left >= 0 && porta.caixa.right <= largura, `${qual}: a porta sai da janela: ${dito}`);
      }

      // 4. Cada porta do lado do que ela abre.
      const pessoas = portas.find((p) => p.texto === "PESSOAS");
      const canais = portas.find((p) => p.texto === "SALAS E CANAIS");
      assert.ok(Math.abs(pessoas.caixa.right - (nav.caixa.right - nav.recuo)) <= 1, `${onde}: a porta PESSOAS não encosta na borda direita da barra, que é o lado da gaveta dela (acaba em ${Math.round(pessoas.caixa.right)} de ${Math.round(nav.caixa.right)}): ${dito}`);
      if (canais) {
        assert.ok(Math.abs(canais.caixa.left - (nav.caixa.left + nav.recuo)) <= 1, `${onde}: a porta SALAS E CANAIS não encosta na borda esquerda da barra: ${dito}`);
      }

      // 5. E elas continuam abrindo as gavetas; a de pessoas abre embaixo da porta dela.
      await page.locator(".sessao-navegacao .linha-pessoas").click();
      assert.equal(await page.locator(".sessao-navegacao .linha-pessoas").getAttribute("aria-expanded"), "true", `${onde}: a porta PESSOAS não abriu a gaveta`);
      const gaveta = await page.locator(".painel-pessoas").evaluate((no) => { const r = no.getBoundingClientRect(); return { left: r.left, right: r.right, visivel: no.getClientRects().length > 0 }; });
      const centro = (pessoas.caixa.left + pessoas.caixa.right) / 2;
      assert.ok(gaveta.visivel && centro >= gaveta.left && centro <= gaveta.right, `${onde}: a porta PESSOAS (centro em ${Math.round(centro)}) não fica em cima da gaveta que ela abre (${Math.round(gaveta.left)}–${Math.round(gaveta.right)})`);
      if (process.env.SEELE_PREVIEW_DIR) {
        await page.screenshot({ path: path.join(process.env.SEELE_PREVIEW_DIR, `navegacao-pessoas-abertas-${largura}x${altura}.png`) });
      }
      await page.keyboard.press("Escape");
      assert.equal(await page.locator(".sessao-navegacao .linha-pessoas").getAttribute("aria-expanded"), "false", `${onde}: Esc não fechou a gaveta de pessoas`);
      if (canais) {
        await page.locator(".sessao-navegacao .linha-canais").click();
        assert.equal(await page.locator(".painel-canais").isVisible(), true, `${onde}: a porta SALAS E CANAIS não abriu a gaveta`);
        await page.keyboard.press("Escape");
        assert.equal(await page.locator(".sessao-navegacao .linha-canais").getAttribute("aria-expanded"), "false", `${onde}: Esc não fechou a gaveta de canais`);
      }

      assert.deepEqual(erros, [], `${onde}: a página lançou: ${erros.join(" | ")}`);
      console.log(`${onde}: barra ${Math.round(nav.caixa.left)}–${Math.round(nav.caixa.right)} × ${Math.round(nav.caixa.height)} px, fundo ${nav.fundo}, raio ${nav.raio}; ${portas.map((p) => `«${p.texto}» ${Math.round(p.caixa.left)}–${Math.round(p.caixa.right)}, contraste ${p.contraste}:1`).join("; ")}`);
      await page.close();
    }
    console.log("navegação estreita: ok");
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro);
  process.exit(1);
});
