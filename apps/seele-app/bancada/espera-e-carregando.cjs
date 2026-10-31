// **A caixa de hospedando e o aviso de sair da espera** (0.19, pedido do dono:
// «tela de loading quando clicar em hospedar um server, e um aviso quando uma
// pessoa tentar sair da tela de espera quando aguarda aprovação, para ela não
// pensar que deu errado»). Com o HTML e os scripts reais; a ponte do Tauri é um
// duble que segura o `hospedar` e o `connect` até a bancada soltá-los.
//
// # O que ela mede
//
// 1. **Hospedando, subindo**: do `hospedar()` até o `invoke("hospedar")`
//    voltar, a caixa está na frente com o passo 1 andando e o 2 por começar, a
//    entrada fica `inert`, e um segundo `hospedar()` não chama o Rust de novo.
// 2. **Hospedando, entrando**: com o servidor no ar, o passo 1 fecha com o
//    alcance que o Rust devolveu e o 2 anda com a etapa de verdade do
//    `connect` (`ConnectStageChanged`).
// 3. **A entrada que parou** vira falha dita na caixa, com a frase da linha
//    vermelha e a nota de que o servidor ficou no ar.
// 4. **O servidor que não subiu**: a frase do erro, e «nada ficou no ar» porque
//    o `meu_servidor` respondeu que nada ficou. FECHAR devolve a entrada.
// 5. **A entrada que deu certo** fecha a caixa sozinha.
// 6. **A espera da portaria no desktop**: a tela diz que esperar é normal; o
//    VOLTAR À ENTRADA e o Esc perguntam antes, com o foco no CONTINUAR
//    ESPERANDO; Esc e apertar fora ficam; SAIR MESMO ASSIM sai e para de bater;
//    a recusa por baixo da caixa a fecha; recusado, sair não pergunta.
// 7. **A espera no celular** («ENTRAR EM»): o aviso é o laranja do que se lê, e
//    não o vermelho do erro; o AGORA NÃO e o ‹ VOLTAR perguntam antes.
//
// Rodar:  node apps/seele-app/bancada/espera-e-carregando.cjs [--fotos <pasta>]
//         [--reverter=carregando|um-por-vez|falha|sair|celular]
//
// `--reverter=<nome>` serve à página uma cópia com o conserto desfeito (nunca
// na árvore), e a bancada tem de reprovar.

"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { servir, respostas, retrato } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const argumentos = process.argv.slice(2);
const fotos = argumentos.includes("--fotos") ? argumentos[argumentos.indexOf("--fotos") + 1] : null;
const reverter = (argumentos.find((a) => a.startsWith("--reverter=")) ?? "").slice("--reverter=".length) || null;

/** Cada reversão: o arquivo servido, o conserto e o que fica no lugar dele. */
const REVERSOES = {
  carregando: ["tela-boot.js", "  abrirHospedando();\n", "\n"],
  "um-por-vez": ["tela-boot.js", "  if (hospedandoAgora) {", "  if (false) {"],
  falha: ["tela-boot.js", 'hospedagemFalhou(subiu ? "entrar" : "subir", erro.textContent, noAr ? f.ficouNoAr : f.nadaNoAr);', "fecharHospedando();"],
  sair: ["tela-auth.js", '$("auth-voltar").addEventListener("click", sairDaEspera);', '$("auth-voltar").addEventListener("click", voltarParaAEntrada);'],
  celular: ["celular-entrada.js", 'const sair = naPortaria ? () => perguntarAntesDeSairDaEspera("aparelho", agoraNao) : agoraNao;', "const sair = agoraNao;"],
};
if (reverter && !REVERSOES[reverter]) {
  console.error(`--reverter: «${reverter}» não é uma das reversões (${Object.keys(REVERSOES).join(", ")})`);
  process.exit(2);
}

async function servirRevertido(page) {
  if (!reverter) return;
  const [arquivo, conserto, desfeito] = REVERSOES[reverter];
  const fonte = fs.readFileSync(path.join(__dirname, "../ui", arquivo), "utf8");
  if (!fonte.includes(conserto)) throw new Error(`--reverter=${reverter}: o conserto não foi achado em ${arquivo}`);
  await page.route(`**/${arquivo}`, (rota) =>
    rota.fulfill({ contentType: "text/javascript", body: fonte.replace(conserto, desfeito) }));
}

async function foto(page, nome) {
  if (!fotos) return;
  fs.mkdirSync(fotos, { recursive: true });
  await page.screenshot({ path: path.join(fotos, nome) });
}

const ANFITRIAO = {
  aqui: "127.0.0.1:8383",
  convite: "seele://casa.exemplo:8383#bancada",
  alcance: "PortaNoRoteador",
  porta_recusada: null,
  encontro_recusado: null,
  firewall: null,
  porta_nao_canonica: null,
};

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const endereco = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium().launch({ headless: true });
  try {
    // ------------------------------------------------------------- o desktop
    const abrirDesktop = async () => {
      const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
      const erros = [];
      page.on("pageerror", (e) => erros.push(e.message));
      await servirRevertido(page);
      await page.addInitScript(({ tabela, snapshot }) => {
        window.chamadas = [];
        window.ouvintes = [];
        // Cada `hospedar`/`connect` fica pendente até a bancada soltar.
        window.pendentes = { hospedar: [], connect: [] };
        window.emitir = (payload) => { for (const o of ouvintes) o({ payload }); };
        window.soltar = (cmd, como, valor) => {
          const p = pendentes[cmd].shift();
          if (como === "ok") p.ok(valor); else p.falha(valor);
        };
        window.snapshotDaBancada = snapshot;
        window.__TAURI__ = {
          core: { invoke: async (cmd, args) => {
            chamadas.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
            if (cmd === "hospedar" || cmd === "connect") {
              return new Promise((ok, falha) => pendentes[cmd].push({ ok, falha }));
            }
            return tabela[cmd] ?? null;
          } },
          event: { listen: async (nome, ouvinte) => { if (nome === "seele://event") ouvintes.push(ouvinte); return () => {}; } },
          window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
        };
      }, { tabela: respostas({ meu_servidor: null, onde_estou: "Guardado", snapshot: null }), snapshot: retrato() });
      await page.goto(endereco);
      await page.waitForTimeout(300);
      return { page, erros };
    };
    const caixa = (page) => page.evaluate(() => ({
      visivel: !$("hospedando").hidden,
      fase: $("hospedando").dataset.fase ?? null,
      titulo: $("hospedando-titulo").textContent,
      subir: [$("hospedando-subir").dataset.estado, $("hospedando-subir-detalhe").textContent],
      entrar: [$("hospedando-entrar").dataset.estado, $("hospedando-entrar-detalhe").textContent],
      erro: $("hospedando-erro").hidden ? null : $("hospedando-erro").textContent,
      nota: $("hospedando-nota").textContent,
      inerte: $("tela-boot").inert,
      fechar: !$("hospedando-fechar").hidden,
      hospedares: chamadas.filter((c) => c.cmd === "hospedar").length,
    }));
    const F = await (async () => {
      const { page } = await abrirDesktop();
      const f = await page.evaluate(() => ({ h: FRASES_DA_HOSPEDAGEM.carregando, e: FRASES_DA_ESPERA }));
      await page.close();
      return f;
    })();

    // 1. Subindo.
    let { page, erros } = await abrirDesktop();
    assert.equal((await caixa(page)).visivel, false, "a caixa de hospedando nasce aberta");
    await page.evaluate(() => { hospedar(); hospedar(); });
    await page.waitForFunction(() => pendentes.hospedar.length > 0);
    let c = await caixa(page);
    assert.equal(c.visivel, true, "o HOSPEDAR não mostrou nada enquanto o servidor subia");
    assert.equal(c.titulo, F.h.titulo.subindo);
    assert.deepEqual(c.subir, ["andando", F.h.subirAndando]);
    assert.deepEqual(c.entrar, ["antes", F.h.entrarAntes]);
    assert.equal(c.inerte, true, "a entrada continua clicável por baixo da caixa");
    assert.equal(c.hospedares, 1, `um segundo hospedar() chamou o Rust de novo: ${c.hospedares}`);
    await foto(page, "hospedando-1-subindo.png");

    // 2. Entrando, com a etapa de verdade.
    await page.evaluate((a) => soltar("hospedar", "ok", a), ANFITRIAO);
    await page.waitForFunction(() => pendentes.connect.length > 0);
    await page.evaluate(() => emitir({ ConnectStageChanged: { stage: "Tocando", tentativa: tentativas } }));
    // A porta (o link) abre por cima, como no produto; fechada, vê-se a caixa.
    assert.equal(await page.evaluate(() => !$("porta").hidden), true, "a porta não abriu por cima da caixa");
    await page.click("#porta-entendi");
    c = await caixa(page);
    assert.equal(c.titulo, F.h.titulo.entrando);
    assert.equal(c.subir[0], "feito");
    assert.ok(c.subir[1].startsWith(F.h.subirFeito) && c.subir[1].length > F.h.subirFeito.length, `o passo 1 não diz o alcance: ${c.subir[1]}`);
    assert.deepEqual(c.entrar, ["andando", "CHAMANDO"], `o passo 2 não diz a etapa do connect: ${c.entrar}`);
    await foto(page, "hospedando-2-entrando.png");

    // 3. A entrada que parou.
    await page.evaluate(() => soltar("connect", "falha", { error: { Refused: { reason: "NicknameTaken" } }, trail: [] }));
    await page.waitForFunction(() => $("hospedando").dataset.fase === "falhou");
    c = await caixa(page);
    const linha = await page.evaluate(() => $("boot-erro").textContent);
    assert.equal(c.titulo, F.h.titulo.naoEntrou);
    assert.equal(c.entrar[0], "falhou");
    assert.equal(c.erro, linha, "a caixa não repete a frase da entrada que parou");
    assert.equal(c.nota, F.h.ficouNoAr);
    assert.equal(c.fechar, true);
    await foto(page, "hospedando-3-entrada-parou.png");
    assert.deepEqual(erros, []);
    await page.close();

    // 4. O servidor que não subiu.
    ({ page, erros } = await abrirDesktop());
    await page.evaluate(() => { hospedar(); });
    await page.waitForFunction(() => pendentes.hospedar.length > 0);
    await page.evaluate(() => soltar("hospedar", "falha", "NaoSubiu"));
    await page.waitForFunction(() => $("hospedando").dataset.fase === "falhou", null, { timeout: 3000 });
    c = await caixa(page);
    assert.equal(c.titulo, F.h.titulo.naoSubiu);
    assert.equal(c.subir[0], "falhou");
    assert.equal(c.entrar[0], "antes");
    assert.equal(c.erro, await page.evaluate(() => fraseDeErro("NaoSubiu")));
    assert.equal(c.nota, F.h.nadaNoAr, "a nota não diz o que o meu_servidor respondeu");
    assert.equal(await page.evaluate(() => document.activeElement?.id), "hospedando-fechar");
    await foto(page, "hospedando-4-nao-subiu.png");
    await page.click("#hospedando-fechar");
    c = await caixa(page);
    assert.equal(c.visivel, false);
    assert.equal(c.inerte, false, "FECHAR deixou a entrada inerte");
    assert.equal(await page.evaluate(() => !$("boot-erro").hidden), true, "a linha vermelha da entrada sumiu junto");
    assert.deepEqual(erros, []);
    await page.close();

    // 5. A entrada que deu certo.
    ({ page, erros } = await abrirDesktop());
    await page.evaluate(() => { hospedar(); });
    await page.waitForFunction(() => pendentes.hospedar.length > 0);
    await page.evaluate((a) => soltar("hospedar", "ok", a), ANFITRIAO);
    await page.waitForFunction(() => pendentes.connect.length > 0);
    await page.evaluate(() => soltar("connect", "ok", { snapshot: snapshotDaBancada, veredito: "Known", geracao: 7 }));
    await page.waitForFunction(() => $("tela-boot").hidden, null, { timeout: 3000 });
    c = await caixa(page);
    assert.equal(c.visivel, false, "a caixa ficou por cima da sessão");
    assert.equal(c.inerte, false);
    assert.deepEqual(erros, []);
    await page.close();

    // 6. A espera da portaria no desktop.
    ({ page, erros } = await abrirDesktop());
    const esperar = async () => {
      await page.evaluate(() => { conectar("198.51.100.7:8383", "Alex"); });
      await page.waitForFunction(() => pendentes.connect.length > 0);
      await page.evaluate(() => soltar("connect", "falha", { error: { Refused: { reason: "AdmissionPending" } }, trail: [] }));
      await page.waitForFunction(() => !$("tela-auth").hidden && $("tela-auth").dataset.espera === "pendente");
    };
    const pergunta = () => page.evaluate(() => ({
      aberta: !$("sair-da-espera").hidden,
      foco: document.activeElement?.id ?? null,
      itens: [...document.querySelectorAll("#sair-da-espera-itens li")].map((li) => li.textContent),
      naEspera: !$("tela-auth").hidden,
    }));
    await esperar();
    assert.equal(await page.evaluate(() => $("auth-espera-normal").textContent), F.e.normal, "a espera não diz que esperar é normal");
    await foto(page, "espera-desktop.png");
    await page.click("#auth-voltar");
    let p = await pergunta();
    assert.equal(p.aberta, true, "VOLTAR À ENTRADA saiu da espera sem perguntar");
    assert.equal(p.foco, "sair-da-espera-ficar", "o padrão não é CONTINUAR ESPERANDO");
    assert.deepEqual(p.itens, [F.e.nadaErrado, F.e.sair, F.e.voltar]);
    assert.equal(p.naEspera, true);
    await foto(page, "sair-da-espera-desktop.png");
    await page.keyboard.press("Escape");
    p = await pergunta();
    assert.equal(p.aberta, false, "Esc não continuou esperando");
    assert.equal(p.naEspera, true, "Esc tirou da espera");
    // Esc na espera é tentar sair, e pergunta.
    await page.keyboard.press("Escape");
    assert.equal((await pergunta()).aberta, true, "Esc na espera saiu sem perguntar");
    await page.mouse.click(8, 8);
    assert.equal((await pergunta()).aberta, false, "apertar fora não continuou esperando");
    // A recusa chega por baixo da pergunta: ela fecha, porque mentiria.
    await page.click("#auth-voltar");
    await page.evaluate(() => levarParaAEspera({ Refused: { reason: "AdmissionDenied" } }, "198.51.100.7:8383", tentativas));
    await page.waitForFunction(() => $("sair-da-espera").hidden, null, { timeout: 2000 });
    // Recusado, sair não pergunta.
    await page.click("#auth-voltar");
    assert.equal((await pergunta()).aberta, false, "uma recusa perguntou antes de sair");
    assert.equal(await page.evaluate(() => !$("tela-boot").hidden), true);
    // De novo na espera: SAIR MESMO ASSIM sai, e para de bater.
    await esperar();
    await page.click("#auth-voltar");
    await page.click("#sair-da-espera-sair");
    assert.equal(await page.evaluate(() => $("tela-auth").hidden && !$("tela-boot").hidden), true, "SAIR MESMO ASSIM não saiu");
    assert.equal(await page.evaluate(() => contagem === null && esperando === false), true, "sair deixou a espera batendo");
    assert.deepEqual(erros, []);
    await page.close();

    // 7. A espera no celular.
    page = await browser.newPage({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
    const errosCelular = [];
    page.on("pageerror", (e) => errosCelular.push(e.message));
    await servirRevertido(page);
    await page.addInitScript(() => localStorage.setItem("seele.casca", "celular"));
    await page.goto(endereco);
    await page.waitForFunction(() => typeof CELULAR !== "undefined" && CELULAR.ajudante("entrada"));
    await page.evaluate(async () => {
      window.desistiu = 0;
      const identidade = criarIdentidadeDoNavegador({
        analisar: () => ({ link: { alvo: "192.0.2.1:8383", impressao: "ab".repeat(32), versao: "2.0" } }),
        conectar: async () => { throw { Refused: { reason: "AdmissionPending" } }; },
      });
      const agoraNao = identidade.agoraNao;
      identidade.agoraNao = () => { desistiu += 1; agoraNao(); };
      await CELULAR.ajudante("entrada").usar(identidade, { link: "link da bancada" });
    });
    await page.locator("#cel-entrada-apelido").fill("bancada celular");
    await page.locator(".cel-entrada-pe button").filter({ hasText: /^ENTRAR$/ }).click();
    const desfecho = page.locator(".cel-entrada-aviso").filter({ hasText: "QUEM HOSPEDA AINDA NÃO DECIDIU" });
    await desfecho.waitFor();
    assert.equal(await desfecho.getAttribute("data-tipo"), null, "a espera do celular veste o vermelho do erro");
    assert.equal(await desfecho.getAttribute("role"), "status");
    await foto(page, "espera-celular.png");
    for (const qual of ["agora-nao", "voltar"]) {
      const saida = qual === "agora-nao" ? page.getByRole("button", { name: "AGORA NÃO", exact: true }) : page.locator(".cel-entrada-voltar");
      await saida.click();
      const q = await page.evaluate(() => ({
        aberta: !$("sair-da-espera").hidden,
        foco: document.activeElement?.id ?? null,
        itens: [...document.querySelectorAll("#sair-da-espera-itens li")].map((li) => li.textContent),
        desistiu,
      }));
      assert.equal(q.aberta, true, "sair da espera do celular não perguntou");
      assert.equal(q.desistiu, 0, "a tentativa foi largada antes do sim");
      assert.equal(q.foco, "sair-da-espera-ficar");
      assert.deepEqual(q.itens, [F.e.nadaErrado, F.e.sairCelular, F.e.voltarCelular]);
      if (qual === "voltar") break;
      await foto(page, "sair-da-espera-celular.png");
      await page.click("#sair-da-espera-ficar");
      assert.equal(await page.evaluate(() => $("sair-da-espera").hidden), true);
    }
    await page.click("#sair-da-espera-sair");
    assert.equal(await page.evaluate(() => desistiu), 1, "SAIR MESMO ASSIM não largou a tentativa");
    assert.deepEqual(errosCelular, []);
    await page.close();
  } finally {
    await browser.close();
    server.close();
  }
  console.log(
    "espera e carregando: hospedar mostra os dois passos de verdade, um por vez, e diz a falha de cada um; " +
      "sair de uma espera pendente pergunta antes, no desktop e no celular, com CONTINUAR ESPERANDO de padrão.",
  );
})().catch((e) => { console.error(e); process.exit(1); });
