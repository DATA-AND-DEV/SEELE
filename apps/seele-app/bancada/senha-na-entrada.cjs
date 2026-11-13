// **A senha do servidor, de quem entra** (1.1.1). Com o HTML e os scripts
// reais; a ponte do Tauri é um duble.
//
// O servidor aceita uma senha desde o ADR 0021, e o link não a leva (ADR 0006:
// «quem usa senha digita a senha»). O app não tinha onde digitá-la: quem colava
// o link de um servidor com senha parava em CREDENCIAL RECUSADA. Com HOSPEDAR
// oferecendo COM SENHA logo de cara, esse beco virou o caminho principal.
//
// # O que ela mede, em 720×480, 900×600 e 1600×919
//
// 1. **A credencial recusada oferece o campo**, com rótulo e a nota que não
//    afirma que o servidor tem senha; outra recusa (quem hospeda disse não) e
//    uma falha de rede não o oferecem.
// 2. **Enter no campo entra de novo**, no mesmo servidor, com a senha no
//    `joinSecret` do `connect`; o campo esvazia e a oferta sai enquanto a
//    tentativa anda. Vazio, não chama nada.
// 3. **A senha errada traz a oferta de volta, vazia.**
// 4. **A senha não vai a mais nenhum lugar**: nem ao console, nem a outro
//    comando, nem ao HTML, nem ao armazenamento.
// 5. **Nada fora da janela**: a página não rola de lado, e o campo e o botão,
//    rolados à vista, cabem inteiros nela.
//
// Rodar:  node apps/seele-app/bancada/senha-na-entrada.cjs [pasta-das-capturas]

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { servir, respostas } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const JANELAS = [
  { width: 720, height: 480 },
  { width: 900, height: 600 },
  { width: 1600, height: 919 },
];
const ALVO = "203.0.113.9:8383";
const SENHA = "zeruel-quatorze";
const RECUSADA = { error: { Refused: { reason: "CredentialRejected" } }, trail: [] };

const pasta = process.argv[2] && !process.argv[2].startsWith("--") ? process.argv[2] : null;
if (pasta) fs.mkdirSync(pasta, { recursive: true });

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const abrir = async (viewport) => {
      const page = await browser.newPage({ viewport });
      const erros = [];
      const console_ = [];
      page.on("pageerror", (e) => erros.push(e.message));
      page.on("console", (m) => console_.push(m.text()));
      await page.addInitScript((tabela) => {
        window.chamadas = [];
        // O que o próximo `connect` faz: uma falha a lançar, ou ficar pendente.
        window.proximoConnect = null;
        window.__TAURI__ = {
          core: { invoke: async (cmd, args) => {
            chamadas.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
            if (cmd === "connect") {
              if (proximoConnect) throw proximoConnect;
              return new Promise(() => {});
            }
            return tabela[cmd] ?? null;
          } },
          event: { listen: async () => () => {} },
          window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
        };
      }, respostas());
      await page.goto(`http://127.0.0.1:${server.address().port}`);
      await page.waitForTimeout(300);
      return { page, erros, console_ };
    };

    /** Uma tentativa a `ALVO` que o servidor recusa com `falha`. */
    const tentar = async (page, falha) => {
      await page.evaluate(({ alvo, falha }) => {
        proximoConnect = falha;
        chamadas.length = 0;
        return conectar(alvo, "Alex");
      }, { alvo: ALVO, falha });
      await page.waitForFunction(() => !$("boot-erro").hidden && $("boot-erro").textContent !== "");
    };

    const oferta = (page) => page.evaluate(() => ({
      aVista: !$("boot-senha").hidden,
      frase: $("boot-erro").textContent,
      rotulo: $("boot-senha-valor").labels?.[0]?.querySelector(".rotulo")?.textContent,
      tipo: $("boot-senha-valor").type,
      nota: $("boot-senha-nota").textContent,
      descrito: $("boot-senha-valor").getAttribute("aria-describedby"),
      valor: $("boot-senha-valor").value,
    }));

    for (const viewport of JANELAS) {
      const tamanho = `${viewport.width}x${viewport.height}`;
      const { page, erros, console_ } = await abrir(viewport);

      // 1. Só a credencial recusada oferece o campo.
      await tentar(page, { error: { Refused: { reason: "AdmissionDenied" } }, trail: [] });
      assert.equal((await oferta(page)).aVista, false, `${tamanho}: a recusa de quem hospeda ofereceu o campo da senha`);
      await tentar(page, { error: "Unreachable", trail: [] });
      assert.equal((await oferta(page)).aVista, false, `${tamanho}: uma falha de rede ofereceu o campo da senha`);

      await tentar(page, RECUSADA);
      let o = await oferta(page);
      assert.equal(o.aVista, true, `${tamanho}: a credencial recusada não ofereceu o campo da senha`);
      assert.match(o.frase, /CREDENCIAL RECUSADA/);
      assert.deepEqual([o.rotulo, o.tipo, o.descrito], ["SENHA DO SERVIDOR", "password", "boot-senha-nota"], `${tamanho}: o campo não tem rótulo, ou não é de senha`);
      assert.match(o.nota, /^Se este servidor tem senha/, `${tamanho}: a nota afirma o que o app não sabe: ${o.nota}`);

      // 5. Nada fora da janela.
      const fora = await page.evaluate(() => {
        const problemas = [];
        const doc = document.documentElement;
        if (doc.scrollWidth > window.innerWidth) problemas.push(`a página rola de lado (${doc.scrollWidth})`);
        for (const id of ["boot-senha-valor", "boot-senha-entrar", "boot-senha-nota"]) {
          const el = $(id);
          el.scrollIntoView({ block: "nearest" });
          const c = el.getBoundingClientRect();
          if (c.width === 0 || c.height === 0) problemas.push(`${id} não está desenhado`);
          if (c.left < -0.5 || c.right > window.innerWidth + 0.5 || c.top < -0.5 || c.bottom > window.innerHeight + 0.5) {
            problemas.push(`${id} não cabe na janela: ${JSON.stringify(c)}`);
          }
          const topo = document.elementFromPoint(c.left + c.width / 2, c.top + c.height / 2);
          if (topo && !el.contains(topo) && !topo.contains(el)) problemas.push(`${id} está debaixo de ${topo.id || topo.className}`);
        }
        return problemas;
      });
      assert.deepEqual(fora, [], tamanho);
      if (pasta) {
        await page.evaluate(() => $("boot-erro").scrollIntoView({ block: "center" }));
        await page.screenshot({ path: path.join(pasta, `senha-na-entrada-${tamanho}.png`) });
      }

      // 2. Vazio não chama nada; com a senha, Enter entra de novo com ela.
      await page.evaluate(() => { chamadas.length = 0; proximoConnect = null; });
      await page.focus("#boot-senha-valor");
      await page.keyboard.press("Enter");
      await page.waitForTimeout(150);
      assert.deepEqual(await page.evaluate(() => chamadas.filter((c) => c.cmd === "connect")), [], `${tamanho}: o campo vazio entrou`);
      await page.fill("#boot-senha-valor", SENHA);
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => chamadas.some((c) => c.cmd === "connect"));
      const entrada = await page.evaluate(() => chamadas.filter((c) => c.cmd === "connect").map((c) => [c.args.server, c.args.joinSecret]));
      assert.deepEqual(entrada, [[ALVO, SENHA]], `${tamanho}: a senha não foi ao connect do servidor recusado`);
      o = await oferta(page);
      assert.deepEqual([o.aVista, o.valor], [false, ""], `${tamanho}: a oferta ficou na tela, ou a senha ficou no campo, com a tentativa andando`);

      // 4. A senha não vai a mais nenhum lugar.
      const vazou = await page.evaluate((senha) => {
        const onde = [];
        for (const c of chamadas) {
          const args = { ...(c.args ?? {}) };
          if (c.cmd === "connect") delete args.joinSecret;
          if (JSON.stringify(args).includes(senha)) onde.push(`o comando ${c.cmd}`);
        }
        if (document.documentElement.outerHTML.includes(senha)) onde.push("o HTML da página");
        for (const guarda of [localStorage, sessionStorage]) {
          for (let i = 0; i < guarda.length; i += 1) {
            if ((guarda.getItem(guarda.key(i)) ?? "").includes(senha)) onde.push(`o armazenamento (${guarda.key(i)})`);
          }
        }
        return onde;
      }, SENHA);
      assert.deepEqual(vazou, [], `${tamanho}: a senha foi parar em: ${vazou.join(", ")}`);

      // 3. A senha errada: a oferta volta, vazia.
      await tentar(page, RECUSADA);
      o = await oferta(page);
      assert.deepEqual([o.aVista, o.valor], [true, ""], `${tamanho}: depois da senha errada a oferta não voltou vazia`);

      assert.deepEqual(console_.filter((linha) => linha.includes(SENHA)), [], `${tamanho}: a senha foi ao console`);
      assert.deepEqual(erros, [], `${tamanho}: erro de página`);
      await page.close();
    }
  } finally {
    await browser.close();
    server.close();
  }
  console.log("a senha na entrada: a credencial recusada oferece o campo, Enter entra de novo com a senha só no connect, e nada sai da janela em 720×480, 900×600 e 1600×919.");
})().catch((e) => { console.error(e); process.exit(1); });
