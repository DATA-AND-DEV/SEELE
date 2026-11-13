// **A faixa de quem está ligando cabe numa linha, e os dois botões andam juntos.**
//
// Relato do dono na 1.0.0 (13/11/2026), janela de 900×1600: a faixa
// «“labubonico” · 0DK4-4J40-JHGD-YAPE está ligando» tinha 450 px numa janela de
// 900, com o ATENDER ao lado do texto e o RECUSAR caído sozinho numa segunda
// linha. A faixa era centrada por `left: 50%` + `translateX(-50%)`: um elemento
// fixo assim só pode ter a largura que sobra da metade da janela para a
// direita, e por isso quebrava com a janela inteira livre.
//
// # O que é simulado, e o que é o produto
//
// SIMULADO: a ponte (`window.__TAURI__` com respostas combinadas) e o evento
// `seele://toque`, entregue como o Rust o entrega.
// REAL: o `index.html`, todo o CSS e todo o JS de `ui/`.
//
// Rodar:  node apps/seele-app/bancada/faixa-do-toque.cjs

const assert = require("node:assert/strict");
const { servir, respostas } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const JANELAS = [[1600, 919], [900, 1600], [760, 560], [420, 800], [360, 640]];
const QUEM = [
  { apelido: "labubonico", nome_na_agenda: null },
  // O caso largo: o nome da agenda e o apelido, os dois compridos.
  { apelido: "Fulano de Tal dos Anzóis", nome_na_agenda: "Beatriz de Albuquerque Montenegro" },
];

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    for (const [largura, altura] of JANELAS) {
      for (const quem of QUEM) {
        const page = await browser.newPage({ viewport: { width: largura, height: altura } });
        const erros = [];
        page.on("pageerror", (e) => erros.push(e.message));
        await page.addInitScript((tabela) => {
          window.tabela = tabela;
          window.__TAURI__ = {
            core: { invoke: async (comando) => window.tabela[comando] ?? null },
            event: { listen: async (nome, f) => {
              (window.__ouvintes ??= {})[nome] = [...(window.__ouvintes[nome] ?? []), f];
              return () => {};
            } },
            window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
          };
        }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
        await page.goto(`http://127.0.0.1:${server.address().port}`);
        await page.waitForTimeout(300);
        await page.evaluate((q) => {
          for (const ouvir of window.__ouvintes?.["seele://toque"] ?? []) {
            ouvir({ payload: { evento: "Tocou", quem: "aa11bb22", codigo: "0DK4-4J40-JHGD-YAPE", com_som: true, ...q } });
          }
        }, quem);
        await page.waitForFunction(() => !document.getElementById("toque").hidden, null, { timeout: 5000 });
        await page.waitForTimeout(150);

        const m = await page.evaluate(() => {
          const r = (id) => { const b = document.getElementById(id).getBoundingClientRect(); return { x: b.left, d: b.right, y: b.top, b: b.bottom, w: b.width, h: b.height }; };
          const texto = document.getElementById("toque-texto");
          const linha = parseFloat(getComputedStyle(texto).lineHeight) || texto.getBoundingClientRect().height;
          return {
            faixa: r("toque"), texto: r("toque-texto"), atender: r("toque-atender"), recusar: r("toque-recusar"),
            linhasDoTexto: Math.round(texto.getBoundingClientRect().height / linha),
            dito: texto.textContent, cortado: texto.scrollWidth > texto.clientWidth + 1,
            janela: innerWidth, rolagem: document.documentElement.scrollWidth > innerWidth + 1,
          };
        });
        const onde = `${largura}×${altura}, «${quem.apelido}»`;
        const mesmaLinha = (a, b) => Math.abs((a.y + a.b) / 2 - (b.y + b.b) / 2) < 6;

        assert.ok(m.faixa.x >= -0.5 && m.faixa.d <= m.janela + 0.5 && !m.rolagem, `${onde}: a faixa sai da janela: ${JSON.stringify(m.faixa)}`);
        // Centrada: a mesma folga dos dois lados.
        assert.ok(Math.abs(m.faixa.x - (m.janela - m.faixa.d)) < 2, `${onde}: a faixa não está centrada: ${JSON.stringify(m.faixa)}`);
        // **Os dois botões na mesma linha, sempre**: o RECUSAR não cai sozinho.
        assert.ok(mesmaLinha(m.atender, m.recusar),
          `${onde}: o ATENDER e o RECUSAR ficaram em linhas diferentes: ${JSON.stringify({ atender: m.atender, recusar: m.recusar, faixa: m.faixa })}`);
        assert.ok(m.atender.d <= m.recusar.x + 0.5, `${onde}: o RECUSAR veio antes do ATENDER`);
        assert.ok(m.recusar.d <= m.faixa.d + 0.5 && m.atender.x >= m.faixa.x - 0.5, `${onde}: um botão sai da faixa`);
        assert.ok(!m.cortado, `${onde}: o texto da faixa está cortado: «${m.dito}»`);
        // **Com a janela larga o aviso curto é uma linha só**: texto e botões.
        // (No relato, a faixa tinha 450 px numa janela de 900.)
        if (largura >= 900 && !quem.nome_na_agenda) {
          assert.equal(m.linhasDoTexto, 1, `${onde}: o texto quebrou em ${m.linhasDoTexto} linhas com a janela larga`);
          assert.ok(mesmaLinha(m.texto, m.atender),
            `${onde}: sobra janela e os botões estão abaixo do texto: ${JSON.stringify({ faixa: m.faixa, texto: m.texto, atender: m.atender })}`);
        }
        assert.deepEqual(erros, [], `${onde}: erros de página: ${erros.join(" | ")}`);
        if (process.env.SEELE_CAPTURAS) {
          await page.screenshot({ path: require("node:path").join(process.env.SEELE_CAPTURAS, `toque-${largura}x${altura}-${quem.nome_na_agenda ? "longo" : "curto"}.png`), clip: { x: 0, y: 0, width: largura, height: Math.min(altura, 260) } });
        }
        await page.close();
      }
    }
    console.log(`ok: a faixa do toque cabe e os botões andam juntos em ${JANELAS.length} janelas, com nome curto e comprido`);
  } finally {
    await browser.close();
    server.close();
  }
})().catch((falha) => { console.error(falha); process.exit(1); });
