// **A senha do servidor, de quem entra pelo link do navegador, contra um
// servidor de verdade** (1.1.1).
//
//   node apps/seele-app/bancada/senha-pelo-link-do-navegador.cjs <seele_web.wasm> <servidor_da_bancada> <seele_motor_web.wasm> <chromium|webkit> <playwright> [--capturas pasta] [--reverter]
//
// O servidor sobe com `--senha`: com senha e sem a portaria, como o app o sobe
// quando quem hospeda escolhe COM SENHA. A página é a `ui/` inteira, com o
// `.wasm` de produto, e o link é o do navegador, que não leva senha (ADR 0006).
//
// Antes do conserto, a identidade da página (`identidade-web.js`) lia a recusa
// da credencial como `convite-gasto` e encerrava a tentativa: quem entrava pela
// página num servidor com senha lia «ESTE CONVITE JÁ FOI USADO» e não tinha
// onde digitar a senha. É como os amigos de quem hospeda entram.
//
// # O que ela prova
//
// 1. **Pela identidade**: sem senha, a recusa é `segredo-recusado` e a tentativa
//    segue; a senha errada é recusada igual; a certa entra, e o retrato é o do
//    servidor.
// 2. **Na casca de celular**: o ENTRAR recusado mostra as duas possibilidades (o
//    convite, ou a senha) e o campo SENHA DO SERVIDOR; a errada devolve o campo
//    vazio; a certa abre a sessão.
// 3. **Na casca larga**: debaixo de CREDENCIAL RECUSADA, o mesmo campo da
//    entrada do app; a errada o devolve vazio; a certa abre a sessão.
// 4. **A senha não fica em lugar nenhum**: nem no endereço, nem no
//    `localStorage`, no `sessionStorage` ou no IndexedDB, nem no console, nem
//    no campo.
//
// `--reverter` devolve à cópia servida a leitura antiga (`convite-gasto`, que
// encerra), e exige que as provas reprovem.

"use strict";

const { spawn } = require("node:child_process");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");
const readline = require("node:readline");

const argumentos = process.argv.slice(2);
const opcao = (nome) => (argumentos.includes(nome) ? argumentos[argumentos.indexOf(nome) + 1] : null);
const capturas = opcao("--capturas");
const reverter = argumentos.includes("--reverter");
const [caminhoDoWasm, caminhoDoServidor, caminhoDoMotor, navegador = "chromium", playwright] = argumentos.filter(
  (a, i) => !a.startsWith("--") && !(i > 0 && argumentos[i - 1] === "--capturas"),
);
if (![caminhoDoWasm, caminhoDoServidor, caminhoDoMotor, playwright].every(Boolean)) {
  console.error("uso: node senha-pelo-link-do-navegador.cjs <seele_web.wasm> <servidor_da_bancada> <seele_motor_web.wasm> <chromium|webkit> <playwright> [--capturas pasta] [--reverter]");
  process.exit(2);
}
if (capturas) fs.mkdirSync(capturas, { recursive: true });
const raiz = path.resolve(__dirname, "..");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");

/** A senha da bancada, e uma errada. Textos que não aparecem em frase nenhuma. */
const SENHA = "zeruel-quatorze";
const ERRADA = "zeruel-quinze!";

const TIPOS = {
  ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm", ".svg": "image/svg+xml", ".png": "image/png", ".json": "application/json", ".woff2": "font/woff2",
};

function servirAPagina() {
  const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "senha-pelo-link-"));
  fs.cpSync(path.join(raiz, "ui"), pasta, { recursive: true });
  fs.copyFileSync(caminhoDoWasm, path.join(pasta, "seele_web.wasm"));
  fs.copyFileSync(caminhoDoMotor, path.join(pasta, "seele_motor_web.wasm"));
  if (reverter) {
    const caminho = path.join(pasta, "identidade-web.js");
    const fonte = fs.readFileSync(caminho, "utf8");
    const sem = fonte.replace('return { erro: "segredo-recusado", segue: true };', 'return { erro: "convite-gasto" };');
    if (sem === fonte) throw new Error("--reverter: a correção não foi achada na cópia; a reversão não reverteu nada");
    fs.writeFileSync(caminho, sem);
  }
  // A página escreve `rtc-web.js` só no build publicado; aqui ele entra à mão,
  // como na `portaria-no-navegador.cjs`.
  const indice = path.join(pasta, "index.html");
  const html = fs.readFileSync(indice, "utf8");
  if (!html.includes('src="rtc-web.js"')) {
    fs.writeFileSync(indice, html.replace('<script src="sessao-web.js"></script>', '<script src="rtc-web.js"></script>\n<script src="sessao-web.js"></script>'));
  }
  return http.createServer((pedido, resposta) => {
    const nome = decodeURIComponent(new URL(pedido.url, "http://x").pathname).replace(/^\/+/, "") || "index.html";
    const arquivo = path.join(pasta, nome);
    if (!arquivo.startsWith(pasta) || !fs.existsSync(arquivo) || fs.statSync(arquivo).isDirectory()) {
      resposta.writeHead(404).end();
      return;
    }
    resposta.writeHead(200, { ...cabecalhosDaOrigem(`/${nome}`), "content-type": TIPOS[path.extname(nome)] ?? "application/octet-stream" });
    resposta.end(fs.readFileSync(arquivo));
  });
}

function subirOServidor() {
  const processo = spawn(caminhoDoServidor, ["--senha", SENHA], { stdio: ["pipe", "pipe", "inherit"] });
  return new Promise((ok, falha) => {
    readline.createInterface({ input: processo.stdout }).once("line", (linha) => ok({ processo, ...JSON.parse(linha) }));
    processo.once("exit", (codigo) => falha(new Error(`o servidor da bancada saiu com ${codigo}`)));
  });
}

/** Tudo o que a origem guarda e o endereço, num texto só, para procurar a senha. */
const oQueAPaginaGuarda = (aba) => aba.evaluate(async () => {
  const partes = [location.href, JSON.stringify({ ...localStorage }), JSON.stringify({ ...sessionStorage })];
  for (const { name } of await indexedDB.databases()) {
    const banco = await new Promise((ok, falha) => {
      const pedido = indexedDB.open(name);
      pedido.onsuccess = () => ok(pedido.result);
      pedido.onerror = () => falha(pedido.error);
    });
    for (const loja of banco.objectStoreNames) {
      const tudo = await new Promise((ok) => {
        const pedido = banco.transaction(loja, "readonly").objectStore(loja).getAll();
        pedido.onsuccess = () => ok(pedido.result);
        pedido.onerror = () => ok([]);
      });
      partes.push(JSON.stringify(tudo));
    }
    banco.close();
  }
  for (const campo of document.querySelectorAll("input")) partes.push(campo.value);
  return partes.join("\n");
});

(async () => {
  const servidor = await subirOServidor();
  const pagina = servirAPagina();
  await new Promise((ok) => pagina.listen(0, "127.0.0.1", ok));
  const endereco = `http://localhost:${pagina.address().port}/`;
  const { chromium, webkit } = require(playwright);
  const browser = await (navegador === "webkit" ? webkit : chromium).launch({
    args: navegador === "webkit" ? [] : ["--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream"],
  });
  const link = servidor.link.replace(/([?&])convite=[^&]*&?/, "$1").replace(/[?&]$/, "");
  const R = { navegador: `${navegador} ${browser.version()}`, reverter, casos: {} };
  const abrir = async (opcoes, caminho) => {
    const contexto = await browser.newContext(opcoes);
    const aba = await contexto.newPage();
    const console_ = [];
    aba.on("console", (m) => console_.push(m.text()));
    await aba.goto(`${endereco}${caminho}`);
    return { contexto, aba, console_ };
  };
  const semASenha = async ({ aba, console_ }) => !(await oQueAPaginaGuarda(aba)).includes(SENHA) && !console_.some((l) => l.includes(SENHA));
  const retratar = async (aba, nome) => {
    if (capturas) await aba.screenshot({ path: path.join(capturas, `${nome}.png`) });
  };

  try {
    // 1. Pela identidade.
    {
      const a = await abrir({}, "index.html");
      R.casos.identidade = await a.aba.evaluate(async ({ link, senha, errada }) => {
        const identidade = await identidadeDaPagina();
        await identidade.abrir(link);
        const curto = (r) => ({ ok: r.ok === true, erro: r.erro ?? null, servidor: r.resposta?.snapshot?.server ?? null });
        const semSenha = curto(await identidade.entrar("ana"));
        const depois = identidade.ler();
        const comErrada = curto(await identidade.entrar("ana", errada));
        const comCerta = curto(await identidade.entrar("ana", senha));
        return { semSenha, estado: depois?.estado ?? null, pede: depois?.credencialRecusada ?? null, comErrada, comCerta };
      }, { link, senha: SENHA, errada: ERRADA });
      R.casos.identidade.semASenha = await semASenha(a);
      await a.contexto.close();
    }

    // 2. A casca de celular, pela tela.
    {
      const c = await abrir({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true }, "index.html");
      const caso = { casca: null, aviso: null, rotulo: null, botoes: null, vazioDepoisDaErrada: null, entrou: false };
      try {
        caso.casca = await c.aba.evaluate(() => document.documentElement.dataset.casca);
        // O link entra colado, como quem o recebe por mensagem: a página desta
        // bancada é `localhost`, e o fragmento na barra seria lido como um link
        // de outra página do SEELE.
        await c.aba.waitForFunction(() => [...document.querySelectorAll(".cel-entrada button")].some((b) => b.textContent === FRASES_DO_CELULAR.entrada.inicio.colar), null, { timeout: 20000 });
        await c.aba.evaluate(() => [...document.querySelectorAll(".cel-entrada button")].find((b) => b.textContent === FRASES_DO_CELULAR.entrada.inicio.colar).click());
        await c.aba.fill("#cel-entrada-link", link);
        await c.aba.press("#cel-entrada-link", "Enter");
        await c.aba.waitForSelector("#cel-entrada-apelido", { timeout: 20000 });
        await c.aba.fill("#cel-entrada-apelido", "bia");
        await c.aba.press("#cel-entrada-apelido", "Enter");
        await c.aba.waitForSelector("#cel-entrada-senha", { timeout: 30000 });
        Object.assign(caso, await c.aba.evaluate(() => ({
          aviso: document.querySelector(".cel-entrada")?.innerText ?? "",
          rotulo: document.querySelector('label[for="cel-entrada-senha"]')?.textContent ?? null,
          botoes: [...document.querySelectorAll(".cel-entrada-pe button")].map((b) => b.textContent),
        })));
        await retratar(c.aba, "celular-pede-a-senha");
        // A senha errada: a tela volta com o campo vazio.
        await c.aba.evaluate(() => { window.__campoDeAntes = document.getElementById("cel-entrada-senha"); });
        await c.aba.fill("#cel-entrada-senha", ERRADA);
        await c.aba.press("#cel-entrada-senha", "Enter");
        await c.aba.waitForFunction(() => {
          const campo = document.getElementById("cel-entrada-senha");
          return campo && campo !== window.__campoDeAntes;
        }, null, { timeout: 30000 });
        caso.vazioDepoisDaErrada = await c.aba.evaluate(() => document.getElementById("cel-entrada-senha").value === "");
        // A certa abre a sessão: a entrada some.
        await c.aba.fill("#cel-entrada-senha", SENHA);
        await c.aba.press("#cel-entrada-senha", "Enter");
        await c.aba.waitForFunction(() => document.getElementById("tela-boot").hidden, null, { timeout: 30000 });
        caso.entrou = true;
        await retratar(c.aba, "celular-entrou");
      } catch (falha) {
        caso.falha = String(falha?.message ?? falha).split("\n")[0];
        await retratar(c.aba, "celular-falhou");
      }
      caso.semASenha = await semASenha(c);
      R.casos.celular = caso;
      await c.contexto.close();
    }

    // 3. A casca larga: o `conectar` da entrada, pela ponte, até o servidor.
    {
      const l = await abrir({ viewport: { width: 1280, height: 800 } }, "index.html");
      const caso = { casca: null, frase: null, rotulo: null, nota: null, vazioDepoisDaErrada: null, entrou: false };
      try {
        caso.casca = await l.aba.evaluate(() => document.documentElement.dataset.casca);
        await l.aba.evaluate(async (link) => {
          const convite = await invoke("analisar_convite", { link });
          conectar(convite.alvo, "caio", convite.token ?? null, link).catch(() => {});
        }, link);
        await l.aba.waitForFunction(() => !document.getElementById("boot-senha").hidden, null, { timeout: 30000 });
        Object.assign(caso, await l.aba.evaluate(() => ({
          frase: document.getElementById("boot-erro").textContent,
          rotulo: document.getElementById("boot-senha-valor").labels?.[0]?.querySelector(".rotulo")?.textContent ?? null,
          nota: document.getElementById("boot-senha-nota").textContent,
        })));
        caso.aVista = await l.aba.evaluate(() => {
          const campo = document.getElementById("boot-senha-valor");
          campo.scrollIntoView({ block: "center" });
          const c = campo.getBoundingClientRect();
          const topo = document.elementFromPoint(c.left + c.width / 2, c.top + c.height / 2);
          return topo === campo ? true : `debaixo de ${topo?.id || topo?.className || topo?.tagName}`;
        });
        // A entrada escurece enquanto a tentativa anda, e clareia quando ela acaba.
        await l.aba.waitForTimeout(1200);
        caso.opacidade = await l.aba.evaluate(() => {
          let menor = 1;
          for (let el = document.getElementById("boot-senha-valor"); el; el = el.parentElement) menor = Math.min(menor, Number(getComputedStyle(el).opacity));
          return menor;
        });
        await retratar(l.aba, "larga-pede-a-senha");
        await l.aba.evaluate(() => { window.__tentativasDeAntes = tentativas; });
        await l.aba.fill("#boot-senha-valor", ERRADA);
        await l.aba.press("#boot-senha-valor", "Enter");
        await l.aba.waitForFunction(() => tentativas > window.__tentativasDeAntes && !document.getElementById("boot-senha").hidden, null, { timeout: 30000 });
        caso.vazioDepoisDaErrada = await l.aba.evaluate(() => document.getElementById("boot-senha-valor").value === "");
        await l.aba.fill("#boot-senha-valor", SENHA);
        await l.aba.press("#boot-senha-valor", "Enter");
        await l.aba.waitForFunction(() => document.getElementById("tela-boot").hidden, null, { timeout: 30000 });
        caso.entrou = true;
        await retratar(l.aba, "larga-entrou");
      } catch (falha) {
        caso.falha = String(falha?.message ?? falha).split("\n")[0];
        caso.frase ??= await l.aba.evaluate(() => document.getElementById("boot-erro").textContent).catch(() => null);
        await retratar(l.aba, "larga-falhou");
      }
      caso.semASenha = await semASenha(l);
      R.casos.larga = caso;
      await l.contexto.close();
    }
  } finally {
    await browser.close();
    pagina.close();
    servidor.processo.stdin.end();
  }

  const { identidade: i, celular: c, larga: l } = R.casos;
  R.veredito = {
    aRecusaSegueEPedeASenha: i.semSenha.erro === "segredo-recusado" && i.pede === true && ["pronto", "numa-aba"].includes(i.estado),
    aErradaERecusadaIgual: i.comErrada.erro === "segredo-recusado",
    aCertaEntra: i.comCerta.ok && Boolean(i.comCerta.servidor),
    oCelularDizAsDuasEPedeASenha:
      c.casca === "celular" && /já foi usado/.test(c.aviso ?? "") && /tem senha/.test(c.aviso ?? "")
      && c.rotulo === "SENHA DO SERVIDOR" && (c.botoes ?? []).includes("ENTRAR COM A SENHA"),
    oCelularDevolveOCampoVazio: c.vazioDepoisDaErrada === true,
    oCelularEntra: c.entrou,
    aLargaPedeASenha: l.casca === "larga" && l.aVista === true && l.opacidade === 1 && /CREDENCIAL RECUSADA/.test(l.frase ?? "") && l.rotulo === "SENHA DO SERVIDOR" && /^Se este servidor tem senha/.test(l.nota ?? ""),
    aLargaDevolveOCampoVazio: l.vazioDepoisDaErrada === true,
    aLargaEntra: l.entrou,
    aSenhaNaoFicaEmLugarNenhum: i.semASenha && c.semASenha && l.semASenha,
  };
  console.log(JSON.stringify(R, null, 2));
  process.exit(Object.values(R.veredito).every(Boolean) ? 0 : 1);
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
