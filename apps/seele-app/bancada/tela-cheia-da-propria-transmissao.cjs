// **Quem transmite põe a própria tela em tela cheia, e ela fica.**
//
// Relato do dono na 1.0.0 (13/11/2026, Windows): «quando eu estou
// compartilhando uma tela e clico duas vezes para ficar em tela cheia, ele sai
// automaticamente». Quem transmite não assiste a si mesmo pelo servidor: o que
// ele vê no palco é o espelho local, e o retrato dele tem `tela: null` (a que
// se ASSISTE) e `minha_transmissao` preenchida. O `desenharPalco` lia só a
// primeira, dizia ao cinema que não havia imagem, e o redesenho seguinte
// (meio segundo depois) tirava a tela cheia.
//
// # O que é simulado, e o que é o produto
//
// SIMULADO: a ponte (`window.__TAURI__` com respostas combinadas; o
// `tela_cheia` responde `null`, como o comando do Rust, que só troca a janela),
// o retrato e o evento `ScreenOpened` do espelho. A imagem do canvas é pintada
// à mão: o decodificador não entra.
// REAL: o `index.html`, todo o CSS e todo o JS de `ui/` — a abertura da imagem,
// o duplo clique, o botão e o redesenho são os do produto.
//
// Rodar:  node apps/seele-app/bancada/tela-cheia-da-propria-transmissao.cjs

const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");
const pw = require("./playwright.cjs");

const EU = 1;
const MINHA = { tela: 7, de: EU, e_minha: true, espectadores: 0, parada: null, pedido: null, codificador: null };

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  try {
    for (const motor of ["chromium", "webkit"]) {
      const browser = await pw[motor]().launch({ headless: true });
      try {
        const page = await browser.newPage({ viewport: { width: 1600, height: 919 } });
        const erros = [];
        page.on("pageerror", (e) => erros.push(e.message));
        await page.addInitScript((tabela) => {
          window.tabela = tabela;
          window.pedidos = [];
          window.ouvintes = [];
          window.__TAURI__ = {
            core: {
              invoke: async (comando, args) => {
                window.pedidos.push([comando, args ?? null]);
                // O comando do Rust devolve `()`: a janela troca, e mais nada.
                if (comando === "tela_cheia") return null;
                return window.tabela[comando] ?? null;
              },
            },
            event: { listen: async (nome, f) => { window.ouvintes.push([nome, f]); return () => {}; } },
            window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
          };
        }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
        await page.goto(`http://127.0.0.1:${server.address().port}`);
        await page.waitForTimeout(400);

        const sozinha = retrato({
          tela: null,
          minha_transmissao: MINHA,
          transmissoes: [{ ...MINHA, assistida: false }],
        });
        await page.evaluate(async (r) => {
          window.tabela.snapshot = r;
          $("tela-boot").hidden = true;
          $("tela-sessao").hidden = false;
          desenhar(r);
          await abrirChamada();
          // O espelho abre como no produto: o núcleo avisa, e o palco arma.
          for (const [nome, f] of window.ouvintes) {
            if (nome === "seele://event") f({ payload: { ScreenOpened: { screen: 7, width: 1280, height: 720, source: "screen" } } });
          }
        }, sozinha);
        await page.waitForTimeout(300);
        // A imagem do espelho, pintada à mão (o decodificador não entra aqui).
        await page.evaluate(() => {
          const c = $("palco-imagem");
          c.hidden = false; c.width = 1280; c.height = 720;
          const ctx = c.getContext("2d"); ctx.fillStyle = "#2040e0"; ctx.fillRect(0, 0, 1280, 720);
        });
        await page.evaluate(() => atualizarChamada());
        await page.waitForTimeout(100);

        const estado = () => page.evaluate(() => ({
          cinema: document.body.dataset.cinema ?? null,
          botaoEscondido: $("palco-cheia").hidden,
          botao: $("palco-cheia").textContent.trim(),
          palcoEscondido: $("palco").hidden,
          imagemEscondida: $("palco-imagem").hidden,
          quem: $("palco-quem").textContent.trim(),
          sair: !!document.getElementById("palco-sair-desktop"),
        }));

        const antes = await estado();
        assert.equal(antes.palcoEscondido, false, `${motor}: o palco de quem transmite sumiu: ${JSON.stringify(antes)}`);
        assert.equal(antes.imagemEscondida, false, `${motor}: o espelho não está no palco: ${JSON.stringify(antes)}`);
        // O botão é o caminho que não depende de saber do duplo clique.
        assert.equal(antes.botaoEscondido, false,
          `${motor}: quem transmite vê a própria tela no palco e não tem o botão TELA CHEIA: ${JSON.stringify(antes)}`);
        assert.notEqual(antes.quem, "SEM COMPARTILHAMENTO",
          `${motor}: o palco mostra a tela desta pessoa e diz que não há compartilhamento`);

        // O gesto do relato: duplo clique na imagem.
        await page.locator("#palco-imagem").dblclick();
        await page.waitForTimeout(150);
        assert.equal((await estado()).cinema, "sim", `${motor}: o duplo clique não entrou em tela cheia`);

        // E os redesenhos seguintes, que no produto vêm a cada retrato
        // (`atualizarChamada`, que relê o retrato e redesenha a chamada): três,
        // com o intervalo do produto, e a tela cheia continua.
        for (let volta = 1; volta <= 3; volta += 1) {
          await page.evaluate(() => atualizarChamada());
          await page.waitForTimeout(500);
          const agora = await estado();
          assert.equal(agora.cinema, "sim",
            `${motor}: a tela cheia de quem transmite saiu sozinha no redesenho ${volta}: ${JSON.stringify(agora)}`);
          assert.equal(agora.sair, true, `${motor}: em tela cheia e sem o SAIR DA TELA CHEIA`);
        }
        const trocas = await page.evaluate(() => window.pedidos.filter(([c]) => c === "tela_cheia").map(([, a]) => a.ligada));
        assert.deepEqual(trocas, [true], `${motor}: a janela foi posta e tirada da tela cheia sem ninguém pedir: ${JSON.stringify(trocas)}`);

        // O botão sai, e sai de verdade.
        await page.locator("#palco-sair-desktop").click();
        await page.waitForTimeout(150);
        assert.equal((await estado()).cinema, null, `${motor}: o SAIR DA TELA CHEIA não saiu`);

        // E pelo botão do palco, o mesmo caminho.
        await page.locator("#palco-cheia").click();
        await page.waitForTimeout(150);
        await page.evaluate(() => atualizarChamada());
        await page.waitForTimeout(500);
        assert.equal((await estado()).cinema, "sim", `${motor}: o botão TELA CHEIA entrou e o redesenho tirou`);

        // **A transmissão acaba em tela cheia: aí sim ela sai sozinha**, porque
        // o que sobra é uma janela preta sem nada dizendo como voltar.
        const acabou = retrato({ tela: null, minha_transmissao: null, transmissoes: [] });
        await page.evaluate((r) => {
          window.tabela.snapshot = r;
          for (const [nome, f] of window.ouvintes) {
            if (nome === "seele://event") f({ payload: { ScreenClosed: { screen: 7 } } });
          }
          desenhar(r);
          return atualizarChamada();
        }, acabou);
        // Não no primeiro redesenho (uma reabertura passa por aqui, abaixo), e
        // sim depois da espera de uma reabertura.
        await page.waitForTimeout(400);
        assert.equal((await estado()).cinema, "sim", `${motor}: a tela cheia saiu no primeiro redesenho sem imagem`);
        await page.waitForFunction(() => !document.body.dataset.cinema, null, { timeout: 5000 })
          .catch(() => { throw new Error(`${motor}: a transmissão acabou e a tela cheia ficou, preta`); });
        const depois = await page.evaluate(() => window.pedidos.filter(([c]) => c === "tela_cheia").map(([, a]) => a.ligada));
        assert.equal(depois.at(-1), false, `${motor}: a janela não foi tirada da tela cheia: ${JSON.stringify(depois)}`);

        // ------------------------------------------- quem assiste, e a reabertura
        //
        // A transmissão de outra pessoa troca de resolução: o fluxo fecha e
        // abre de novo, e por um instante o retrato não tem tela. Quem assiste
        // em tela cheia continua nela.
        const DELA = { tela: 9, de: 2, e_minha: false, espectadores: 1, parada: null, pedido: null, codificador: null };
        const vendo = retrato({ tela: DELA, minha_transmissao: null, transmissoes: [{ ...DELA, assistida: true }] });
        const noVao = retrato({ tela: null, minha_transmissao: null, transmissoes: [{ ...DELA, assistida: false }] });
        const abrirADela = () => page.evaluate(() => {
          for (const [nome, f] of window.ouvintes) {
            if (nome === "seele://event") f({ payload: { ScreenOpened: { screen: 9, width: 1280, height: 720, source: "screen" } } });
          }
          const c = $("palco-imagem"); c.hidden = false; c.width = 1280; c.height = 720;
          c.getContext("2d").fillRect(0, 0, 1280, 720);
        });
        const redesenhar = (r) => page.evaluate((r2) => { window.tabela.snapshot = r2; return atualizarChamada(); }, r);
        await redesenhar(vendo);
        await abrirADela();
        await redesenhar(vendo);
        await page.locator("#palco-imagem").dblclick();
        await page.waitForTimeout(150);
        assert.equal((await estado()).cinema, "sim", `${motor}: quem assiste não entrou em tela cheia`);
        // O vão da reabertura: o fluxo fecha, o retrato fica sem tela, e 600 ms
        // depois o novo abre.
        await page.evaluate(() => {
          for (const [nome, f] of window.ouvintes) {
            if (nome === "seele://event") f({ payload: { ScreenClosed: { screen: 9 } } });
          }
        });
        await redesenhar(noVao);
        await page.waitForTimeout(600);
        assert.equal((await estado()).cinema, "sim",
          `${motor}: a transmissão foi reaberta (troca de resolução) e quem assistia saiu da tela cheia`);
        await abrirADela();
        await redesenhar(vendo);
        await page.waitForTimeout(2600);
        const firme = await estado();
        assert.equal(firme.cinema, "sim", `${motor}: a imagem voltou e a tela cheia saiu depois: ${JSON.stringify(firme)}`);
        await page.locator("#palco-sair-desktop").click();
        await page.waitForTimeout(150);
        assert.equal((await estado()).cinema, null, `${motor}: o SAIR DA TELA CHEIA de quem assiste não saiu`);

        assert.deepEqual(erros, [], `${motor}: erros de página: ${erros.join(" | ")}`);
        console.log(`ok: a tela cheia fica para quem transmite e para quem assiste, e sai quando a transmissão acaba (${motor})`);
      } finally {
        await browser.close();
      }
    }
  } finally {
    server.close();
  }
})().catch((falha) => { console.error(falha); process.exit(1); });
