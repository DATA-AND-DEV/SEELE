// **A cópia de segurança antes de converter, só com a comprovação** (E2 da
// 0.17; revisões do Codex sobre a preservação). Com o HTML e os scripts reais;
// a ponte é um duble com a forma do backend (`src/antes_de_hospedar.rs`:
// `antes_de_hospedar`, `preparar_copia`, e as recusas `CopiaDeSeguranca` e
// `DadosNaoVerificados` do `hospedar`).
//
// # A regra
//
// «Cópia do estado atual» só aparece com a comprovação: a cópia que o
// `preparar_copia` acabou de criar e verificar, nesta tela. Toda cópia que já
// estava no disco é histórico (a linha e a data), mesmo com `do_estado_atual`
// na lista: o Codex mediu que inferir pelo nome e pela data erra com o WAL e
// com cópia corrompida, e toda conversão do `hospedar` cria e verifica a sua.
//
// # Os casos, todos com um servidor criado na 0.16 (dados do Servidor 1.0)
//
// 1. **Cópias no disco** (a da 0.15 e uma de segurança que a lista marca como
//    do estado atual): as duas são histórico; nada diz «do estado atual»; o
//    CTA é CRIAR CÓPIA, CONVERTER E HOSPEDAR, habilitado.
// 2. **O CTA chama só o `hospedar`**, do mesmo servidor: a cópia é do portão
//    da conversão dele, e não uma segunda antes.
// 3. **O `hospedar` volta com `CopiaDeSeguranca`**: a tela reabre bloqueada,
//    com a etapa dita, TENTAR A CÓPIA DE NOVO e o motivo cru nos detalhes.
// 4. **TENTAR A CÓPIA DE NOVO dá certo**: só aí «criada e verificada», com a
//    data; o CTA acende. 4b. **E falha na verificação**: continua bloqueado.
// 5. **`verificado: false`**: NÃO FOI POSSÍVEL VERIFICAR, bloqueado, com
//    VERIFICAR DE NOVO e o motivo. 6. **O comando falha**: o mesmo.
// 7. **O `hospedar` volta com `DadosNaoVerificados`**: a tela, bloqueada.
//
// Rodar:  node apps/seele-app/bancada/copia-antes-de-converter.cjs

const assert = require("node:assert/strict");
const { servir, respostas } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

// Sem `esquema_desta_versao`: o número é da build, e não desta bancada.
const BASE = {
  servidor: "srv-16", banco: "servidores/srv-16/seele.db", esquema: { Gravado: 17 }, linha_dos_dados: "1",
  verificado: true, motivo: null, acoes: [], converte: true, futuro: false, vai_copiar: true, mods_de_api_antiga: [],
};
const DO_DISCO = [
  { tipo: "seguranca", linha: "1", origem: "servidores/srv-16/seele.db", esquema: 17, data: 1761300000,
    caminho: "copias-de-seguranca/srv-16/esquema-17-1761300000000.db", do_estado_atual: true },
  { tipo: "linha-legada", linha: "0.15", origem: "seele.db", esquema: null, data: 1700000000,
    caminho: "lancador/dados/servidor-0.15", do_estado_atual: false },
];
const COMPROVADA = {
  tipo: "seguranca", linha: "1", origem: "servidores/srv-16/seele.db", esquema: 17, data: 1761400000,
  caminho: "copias-de-seguranca/srv-16/esquema-17-1761400000000.db", do_estado_atual: true,
};
const LINHAS = [
  { linha: "2", servidor: "2.0", apis: [7, 6], app: null, esta: true, instalada: true, tem_pacote: true },
  { linha: "1", servidor: "1.0", apis: [6], app: "0.16.0", esta: false, instalada: true, tem_pacote: true },
  { linha: "0.15", servidor: "0.15", apis: [5, 4, 3], app: "0.15.2", esta: false, instalada: true, tem_pacote: true },
];

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const erros = [];
    page.on("pageerror", (e) => erros.push(e.message));
    await page.addInitScript((tabela) => {
      window.tabela = tabela;
      window.chamadas = [];
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => {
          chamadas.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
          const r = tabela[cmd];
          if (r && typeof r === "object" && "__lanca" in r) throw r.__lanca;
          return r ?? null;
        } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({
      servidores_guardados: [{ id: "srv-16", nome: "Oficina", versao: "0.16.0", caminho: "servidores/srv-16", ultimo_uso: 0 }],
      linhas_para_hospedar: LINHAS,
    }));
    await page.goto(`http://127.0.0.1:${server.address().port}`);

    const ler = () => page.evaluate(() => ({
      estado: $("antes-dados").dataset.estado,
      texto: $("antes-dados").textContent,
      aviso: $("antes-dados-aviso").hidden ? "" : $("antes-dados-aviso").textContent,
      cta: $("antes-hospedar").textContent,
      bloqueado: $("antes-hospedar").getAttribute("aria-disabled"),
      motivo: $("antes-hospedar-motivo").hidden ? "" : $("antes-hospedar-motivo").textContent,
      descrito: $("antes-hospedar").getAttribute("aria-describedby"),
      acoes: [...$("antes-dados-acoes").querySelectorAll("button")].map((b) => b.textContent),
      detalhes: $("antes-detalhes-lista").textContent,
    }));
    const abrir = async (preconferencia, extra = {}) => {
      await page.evaluate(({ pre, extra }) => {
        chamadas.length = 0;
        tabela.antes_de_hospedar = pre.falha ? { __lanca: pre.falha } : pre;
        Object.assign(tabela, extra);
        window.aberta = hospedarDaLista({ id: "srv-16", nome: "Oficina", versao: "0.16.0", caminho: "servidores/srv-16" });
      }, { pre: preconferencia, extra });
      await page.waitForFunction(() => !$("tela-antes-de-hospedar").hidden && $("antes-dados").dataset.estado);
      await page.waitForTimeout(150);
      return ler();
    };
    const semProva = (t, caso) => {
      assert.doesNotMatch(t.texto, /preservad|estado atual criada|do estado atual feita/i,
        `${caso}: a tela afirma uma cópia do estado atual sem a comprovação: ${t.texto}`);
    };

    // 1. As cópias do disco são histórico, mesmo a que a lista marca.
    let t = await abrir({ ...BASE, copias: DO_DISCO });
    semProva(t, "cópias do disco");
    assert.match(t.texto, /CÓPIAS ANTERIORES/, `o histórico não aparece: ${t.texto}`);
    assert.match(t.texto, /Cópia anterior para o Servidor 1\.0, de [\d/]+\. É histórico/, `a cópia de segurança do disco não é histórico: ${t.texto}`);
    assert.match(t.texto, /Cópia anterior para o Servidor 0\.15, de [\d/]+\. É histórico/, `a da 0.15 não é histórico: ${t.texto}`);
    assert.match(t.aviso, /cria uma cópia de segurança nova/, `o aviso não diz que a conversão cria a sua: ${t.aviso}`);
    assert.equal(t.cta, "CRIAR CÓPIA, CONVERTER E HOSPEDAR", `o CTA não diz a cópia: ${t.cta}`);
    assert.equal(t.bloqueado, "false", "com o portão do backend, o CTA ficou bloqueado");
    assert.equal(t.descrito, "antes-hospedar-motivo", "o motivo não está ligado ao botão");
    assert.doesNotMatch(t.detalhes, /do estado atual/, `os detalhes repetem a inferência do disco: ${t.detalhes}`);

    // 2 e 3. O CTA chama só o hospedar; ele volta com a cópia que não ficou pronta.
    await page.evaluate(() => {
      tabela.hospedar = { __lanca: { CopiaDeSeguranca: { falha: { etapa: "Copiar", motivo: "No space left on device (os error 28)" } } } };
    });
    await page.click("#antes-hospedar");
    await page.waitForFunction(() => chamadas.some((c) => c.cmd === "hospedar"));
    await page.waitForFunction(() => !$("tela-antes-de-hospedar").hidden && $("antes-dados").dataset.estado === "resolva");
    await page.waitForTimeout(150);
    const pedidos = await page.evaluate(() => chamadas.filter((c) => ["preparar_copia", "hospedar"].includes(c.cmd)));
    assert.deepEqual(pedidos.map((c) => c.cmd), ["hospedar"], `o CTA fez outra coisa além do hospedar: ${JSON.stringify(pedidos)}`);
    assert.equal(pedidos[0].args.servidor, "srv-16", "o hospedar foi de outro servidor");
    t = await ler();
    assert.equal(t.bloqueado, "true", "com a cópia que não ficou pronta, o CTA ficou aceso");
    assert.match(t.texto, /NÃO FICOU PRONTA/, `a falha da cópia não é dita: ${t.texto}`);
    assert.match(t.texto, /O disco recusou a cópia/, `a etapa não é dita: ${t.texto}`);
    assert.deepEqual(t.acoes, ["TENTAR A CÓPIA DE NOVO"], `não há TENTAR DE NOVO: ${t.acoes}`);
    assert.match(t.motivo, /Bloqueado: a cópia de segurança falhou/, `o motivo do bloqueio não está ao lado do botão: ${t.motivo}`);
    assert.match(t.detalhes, /Copiar: No space left on device/, `o motivo cru não está nos detalhes: ${t.detalhes}`);
    semProva(t, "a cópia que falhou");

    // 4b. TENTAR DE NOVO, e a verificação reprova: continua bloqueado.
    await page.evaluate(() => { tabela.preparar_copia = { __lanca: { etapa: "Verificar", motivo: "integrity_check: page 3" } }; });
    await page.click("#antes-dados-acoes button");
    await page.waitForFunction(() => chamadas.some((c) => c.cmd === "preparar_copia"));
    await page.waitForTimeout(100);
    t = await ler();
    assert.equal(t.bloqueado, "true", "a cópia que não passou na verificação acendeu o CTA");
    assert.match(t.texto, /não passou na verificação/, `a etapa Verificar não é dita: ${t.texto}`);
    semProva(t, "a cópia que não verificou");

    // 4. TENTAR DE NOVO dá certo: só agora a comprovação.
    await page.evaluate((c) => { tabela.preparar_copia = c; chamadas.length = 0; }, COMPROVADA);
    await page.click("#antes-dados-acoes button");
    await page.waitForFunction(() => $("antes-hospedar").getAttribute("aria-disabled") === "false");
    t = await ler();
    assert.match(t.aviso, /Cópia de segurança do estado atual criada e verificada em [\d/]+, para o Servidor 1\.0/,
      `a comprovação não é dita: ${t.aviso}`);
    assert.match(t.texto, /copias-de-seguranca\/srv-16\/esquema-17-1761400000000\.db/, "não diz onde a cópia ficou");
    assert.match(t.detalhes, /cópia criada e verificada agora/, "os detalhes não têm a comprovação");
    assert.equal(t.cta, "CRIAR CÓPIA, CONVERTER E HOSPEDAR", "a conversão também faz a sua, e o rótulo deixou de dizer");
    assert.deepEqual(await page.evaluate(() => chamadas.find((c) => c.cmd === "preparar_copia").args), { servidor: "srv-16" });

    // 5. Lido, e não verificado.
    await page.evaluate(() => { tabela.hospedar = null; tabela.preparar_copia = null; });
    t = await abrir({ ...BASE, esquema: { Ilegivel: "permission denied (os error 13)" }, linha_dos_dados: null,
      verificado: false, motivo: "permission denied (os error 13)", acoes: ["tentar-de-novo", "abrir-a-pasta"],
      converte: false, vai_copiar: false, copias: [] });
    assert.equal(t.estado, "resolva", `um banco não verificado virou ${t.estado}`);
    assert.match(t.texto, /NÃO FOI POSSÍVEL VERIFICAR OS DADOS/);
    assert.match(t.texto, /permission denied/, "o motivo não aparece");
    assert.equal(t.bloqueado, "true", "sem verificar, o CTA ficou aceso");
    assert.deepEqual(t.acoes, ["VERIFICAR DE NOVO"]);
    semProva(t, "não verificado");

    // 6. O comando falha.
    t = await abrir({ falha: { BancoNaoRespondeu: "database is locked" } });
    assert.equal(t.estado, "resolva");
    assert.equal(t.bloqueado, "true");
    assert.deepEqual(t.acoes, ["VERIFICAR DE NOVO"]);

    // 7. O hospedar volta com DadosNaoVerificados (pela entrada).
    await page.evaluate(() => {
      tabela.antes_de_hospedar = null;
      tabela.hospedar = { __lanca: { DadosNaoVerificados: { motivo: "file is not a database", acoes: ["abrir-a-pasta"] } } };
      $("tela-antes-de-hospedar").hidden = true;
      $("tela-boot").hidden = false;
      servidorEscolhido = "srv-16";
      hospedar();
    });
    await page.waitForFunction(() => !$("tela-antes-de-hospedar").hidden && $("antes-dados").dataset.estado === "resolva");
    await page.waitForTimeout(150);
    t = await ler();
    assert.equal(t.bloqueado, "true", "DadosNaoVerificados deixou o CTA aceso");
    assert.match(t.texto, /NÃO FOI POSSÍVEL VERIFICAR OS DADOS/);
    assert.match(t.texto, /file is not a database/);

    assert.deepEqual(erros, [], `erros na página: ${erros.join(" | ")}`);
  } finally {
    await browser.close();
    server.close();
  }
  console.log("cópia antes de converter: as do disco são histórico; o CTA é o hospedar, com o portão dele; a cópia que não ficou pronta bloqueia com a etapa e TENTAR DE NOVO; só a do preparar_copia é «criada e verificada»; não verificado bloqueia com VERIFICAR DE NOVO.");
})().catch((e) => { console.error(e); process.exit(1); });
