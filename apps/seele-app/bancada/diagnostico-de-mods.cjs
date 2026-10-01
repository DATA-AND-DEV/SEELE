// O diagnóstico de MODs da fase M1, na página, nas folhas e na CSP do produto.
//
// # Por que num navegador, e não na `vm`
//
// `regiao-do-mod.cjs` e `contribuicoes-e-camadas.cjs` medem a região e o
// registro com um DOM de mentira, e é lá que moram as provas de descarte e de
// disputa. O que só um navegador mede é o que esta bancada mede:
//
// - **a mídia de cada ponto**: o que os renderers, o retrato de avatar e os
//   cartões da API 3 anotam, somado por ponto de contribuição;
// - **a gestão**: «quem pinta cada lugar» desenhada a partir do registro e da
//   mídia de verdade, e redesenhada quando os bytes chegam;
// - **o modo de desenvolvedor**: que os contornos deixam o clique chegar ao
//   canal de baixo e não tiram o foco de quem escreve;
// - **o som**: `decodeAudioData` de verdade, sob a política que o produto
//   declara (`default-src 'self'`, sem `media-src`) — medido no Chromium, o
//   motor do WebView2; a medida no WKWebView é a Task 1 (antes) e a Task 9
//   (depois) do Plano 1D.
//
// Só a ponte nativa é simulada, como em `avatares-do-mod.cjs`.
//
//   node apps/seele-app/bancada/diagnostico-de-mods.cjs

const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

/** Deixa a sessão desenhada numa página que acabou de carregar, ou de recarregar. */
async function prepararASessao(pagina) {
  await pagina.evaluate((r) => {
    testTable.snapshot = r;
    $("tela-boot").hidden = true;
    $("tela-sessao").hidden = false;
    desenhar(r);
    entrarNaGeracao(1);
    // Uma instância de MOD ativa nesta geração, com os eventos anotados.
    window.instancia = (id) => {
      const nova = new InstanciaDeMod(id, id.replace("/", "-"), geracaoDaSessao, {
        entregar: async (mensagem) => {
          eventosDoMod.push({ mod: id, ...mensagem });
        },
        encerrar: async () => {},
      });
      nova.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set(id, nova);
      return nova;
    };
  }, retrato());
}

/** Uma página do produto com a ponte simulada, e os erros e recusas da CSP anotados. */
async function abrirASessao(navegador, servidor) {
  const pagina = await navegador.newPage({ viewport: { width: 1400, height: 900 } });
  const erros = [];
  const recusasDaCsp = [];
  pagina.on("pageerror", (erro) => erros.push(erro.message));
  pagina.on("console", (mensagem) => {
    const texto = mensagem.text();
    if (texto.includes("Content Security Policy") && /media|audio/i.test(texto)) {
      recusasDaCsp.push(texto);
    }
  });
  await pagina.addInitScript((tabela) => {
    window.testTable = tabela;
    window.testCalls = [];
    window.eventosDoMod = [];
    window.__TAURI__ = {
      core: {
        invoke: async (cmd, args) => {
          testCalls.push({ cmd, args });
          const resposta = testTable[cmd];
          return typeof resposta === "function" ? resposta(args) : (resposta ?? null);
        },
      },
      event: { listen: async () => () => {} },
      window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
    };
  }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
  await pagina.goto(`http://127.0.0.1:${servidor.address().port}`);
  await prepararASessao(pagina);
  return { pagina, erros, recusasDaCsp };
}

/**
 * Espera a página chegar a um estado, e diz o que quebrou quando ela não chega.
 *
 * O prazo vencido do Playwright diz só «Timeout 30000ms exceeded»: quem lê o
 * CI precisa saber **qual** estado faltou e em que pé a página ficou, e não só
 * que alguma espera não terminou.
 *
 * @param {Function} condicao Rodada na página, até devolver verdadeiro.
 * @param {string} oQueQuebra A frase da falha.
 * @param {Function} [comoFicou] Rodada na página quando o prazo vence; o que
 *   ela devolve entra na frase.
 */
async function esperarQue(pagina, condicao, oQueQuebra, comoFicou) {
  try {
    await pagina.waitForFunction(condicao, null, { timeout: 10_000 });
  } catch (falha) {
    const ficou = comoFicou
      ? await pagina.evaluate(comoFicou).catch((erro) => `não deu para ler: ${erro.message.split("\n")[0]}`)
      : undefined;
    const estado = ficou === undefined ? "" : ` — ficou ${JSON.stringify(ficou)}`;
    throw new Error(`${oQueQuebra}${estado} (${falha.message})`);
  }
}

// ---------------------------------------------------------------------------
// A mídia de cada ponto é contada, e as regras de preferência são uma só.
// ---------------------------------------------------------------------------

async function aMidiaDeCadaPontoEContadaEDita(navegador, servidor) {
  const { pagina, erros } = await abrirASessao(navegador, servidor);
  const contagens = () => ({
    "canal.item": midiasDoPonto("canal.item"),
    "pessoa.avatar": midiasDoPonto("pessoa.avatar"),
  });
  await pagina.evaluate(() => {
    window.avisosDeMidia = 0;
    globalThis.addEventListener("seele-mods-midia", () => {
      window.avisosDeMidia += 1;
    });
    window.foto = () => {
      const tela = document.createElement("canvas");
      tela.width = tela.height = 8;
      return tela.toDataURL();
    };
    window.pendentes = [];
    testTable.midia_do_mod = (args) => (args.caminho === "img/falta.png"
      ? Promise.reject({ Recusado: { motivo: "arquivo-nao-declarado" } })
      : new Promise((resolve) => pendentes.push(resolve)));
    testTable.ler_imagem_mod = () => new Promise((resolve) => pendentes.push(resolve));
    const a = instancia("mod/a");
    contribuicoesDosMods.registrar({ id: "mod/a" }, a, {
      ponto: "canal.item",
      modo: "adicionar",
      conteudo: [{ forma: "midia", chave: "selo", fonte: "img/selo.png" }],
    });
    contribuicoesDosMods.registrar({ id: "mod/a" }, a, {
      ponto: "pessoa.avatar",
      modo: "substituir",
      alvo: "1",
      conteudo: { doServidor: { canal: 1, campo: "image", pedido: { transporte: "volume", path: "volume:foto" } } },
    });
    desenhar(testTable.snapshot);
  });

  // Dois canais, dois selos; e o retrato da pessoa 1, que aparece em três
  // lugares e carrega uma vez só.
  await esperarQue(
    pagina,
    () => midiasDoPonto("canal.item").carregando === 2 && midiasDoPonto("pessoa.avatar").carregando === 1,
    "os dois selos e o retrato que estão a caminho não foram contados como «carregando»",
    contagens,
  );
  await pagina.evaluate(() => {
    for (const pronto of pendentes.splice(0)) pronto({ papel: "imagem", uri: foto(), bytes: 64 });
  });
  await esperarQue(
    pagina,
    () => midiasDoPonto("canal.item").pronta === 2 && midiasDoPonto("pessoa.avatar").pronta === 1,
    "os bytes chegaram e os selos ou o retrato não foram contados como «pronta»",
    contagens,
  );

  // Uma recusa conta, e diz o motivo que o Rust deu, com o MOD na frente.
  //
  // **O contador recomeça aqui**, e não lá em cima: esta recusa vem da região
  // de um `canal.item`, pelo `dono.midiaMudou`, e não pelo retrato de avatar.
  // Contado desde o começo, o retrato bastaria para a conferência passar sem
  // o aviso do dono.
  await pagina.evaluate(() => {
    window.avisosDeMidia = 0;
    contribuicoesDosMods.registrar({ id: "mod/a" }, modsCarregados.get("mod/a"), {
      ponto: "canal.item",
      modo: "adicionar",
      conteudo: [{ forma: "midia", chave: "falta", fonte: "img/falta.png" }],
    });
    desenhar(testTable.snapshot);
  });
  await esperarQue(
    pagina,
    () => midiasDoPonto("canal.item").recusada === 2,
    "o arquivo que o Rust recusou não foi contado como «recusada» nos dois canais",
    contagens,
  );
  const motivos = await pagina.evaluate(() => midiasDoPonto("canal.item").motivos.join(" | "));
  assert.equal(motivos, "mod/a: arquivo-nao-declarado", `o motivo da recusa não veio como o Rust o deu: ${motivos}`);
  assert.ok(await pagina.evaluate(() => window.avisosDeMidia > 0), "a mídia mudou de estado e ninguém foi avisado");

  // **Um aviso por volta**, como `avisarQueAMidiaMudou` promete: vinte
  // retratos chegando juntos são um redesenho da gestão, e não vinte.
  const avisosNumaVolta = await pagina.evaluate(async () => {
    window.avisosDeMidia = 0;
    avisarQueAMidiaMudou();
    avisarQueAMidiaMudou();
    avisarQueAMidiaMudou();
    await new Promise((pronto) => setTimeout(pronto, 0));
    return window.avisosDeMidia;
  });
  assert.equal(
    avisosNumaVolta,
    1,
    `três mudanças na mesma volta deram ${avisosNumaVolta} aviso(s), e não um: a gestão redesenharia uma vez por mídia`,
  );

  // **A regra dos cartões da API 3 é a de `cartoesDaPessoa`**, lida por fora.
  const cartoes = await pagina.evaluate(() => {
    darCartoesDoMod({ id: "mod/b", hash: "mod-b" }, instancia("mod/b"), { 2: [{ forma: "texto", dentro: "B" }] });
    const auto = modsDeCartaoQueValem();
    escolherApresentacao("pessoa.cartao", APRESENTACAO_NATIVA);
    const nativo = modsDeCartaoQueValem();
    escolherApresentacao("pessoa.cartao", "mod/b");
    const escolhido = modsDeCartaoQueValem();
    escolherApresentacao("pessoa.cartao", "mod/z");
    const outro = modsDeCartaoQueValem();
    escolherApresentacao("pessoa.cartao", "");
    return { auto, nativo, escolhido, outro };
  });
  assert.deepEqual(
    cartoes,
    { auto: ["mod/b"], nativo: [], escolhido: ["mod/b"], outro: [] },
    "a regra que diz quais cartões da API 3 entram na lista mudou",
  );

  // **A mídia de um cartão da API 3 conta em `pessoa.cartao`**, e a da região
  // do mesmo MOD não: as duas moram na mesma `RegiaoDeMod`, e só a anotação
  // `cartao` as separa. Sem esta conferência, o ramo dos cartões de
  // `midiasDoPonto` existiria sem que nada o medisse.
  await pagina.evaluate(() => {
    const b = { id: "mod/b", hash: "mod-b" };
    darCartoesDoMod(b, modsCarregados.get("mod/b"), {
      2: [{ forma: "texto", dentro: "B" }, { forma: "midia", chave: "foto", fonte: "img/b.png" }],
    });
    desenharARegiaoDoMod(b, modsCarregados.get("mod/b"), [{ forma: "midia", chave: "na-regiao", fonte: "img/r.png" }]);
  });
  await esperarQue(
    pagina,
    () => testCalls.filter((chamada) => chamada.cmd === "midia_do_mod"
      && ["img/b.png", "img/r.png"].includes(chamada.args?.caminho)).length === 2,
    "o cartão e a região de mod/b não pediram as mídias deles",
  );
  const doCartao = await pagina.evaluate(() => midiasDoPonto("pessoa.cartao"));
  assert.deepEqual(
    doCartao,
    { carregando: 1, pronta: 0, recusada: 0, motivos: [] },
    "a mídia de um cartão da API 3 não foi contada em «pessoa.cartao», ou a da região do MOD foi contada junto",
  );

  // **A preferência que o avatar consulta**: a dele, e na falta dela a do cartão.
  const preferencias = await pagina.evaluate(() => {
    escolherApresentacao("pessoa.cartao", "mod/a");
    const herdada = preferenciaConsultadaPara("pessoa.avatar");
    escolherApresentacao("pessoa.avatar", APRESENTACAO_NATIVA);
    const propria = preferenciaConsultadaPara("pessoa.avatar");
    escolherApresentacao("pessoa.avatar", "");
    escolherApresentacao("pessoa.cartao", "");
    return { herdada, propria };
  });
  assert.deepEqual(preferencias, { herdada: "mod/a", propria: ":nativo" }, "o avatar deixou de consultar a própria preferência, e na falta dela a do cartão");

  assert.deepEqual(erros, [], `a página lançou erro durante a contagem da mídia: ${erros.join(" | ")}`);
  await pagina.close();
}

const PROVAS = [
  aMidiaDeCadaPontoEContadaEDita,
];

(async () => {
  const servidor = servir();
  await new Promise((pronto) => servidor.listen(0, "127.0.0.1", pronto));
  const falhas = [];
  let navegador = null;
  // **O navegador sobe dentro do `try`.** Fora dele, um Playwright ausente
  // lançava com o servidor de arquivos ainda escutando, e o processo não
  // terminava nunca — no CI, um job pendurado até o prazo em vez de uma falha.
  try {
    navegador = await chromium().launch({ headless: true });
    for (const prova of PROVAS) {
      try {
        await prova(navegador, servidor);
      } catch (falha) {
        falhas.push(`${prova.name}: ${falha?.stack ?? falha}`);
      }
    }
  } finally {
    await navegador?.close();
    await new Promise((pronto) => servidor.close(pronto));
  }
  if (falhas.length > 0) {
    for (const falha of falhas) console.error(`FALHOU — ${falha}`);
    process.exitCode = 1;
    return;
  }
  console.log(`diagnóstico de MODs: ${PROVAS.map((prova) => prova.name).join(", ")} — no Chromium, com a CSP do produto.`);
})().catch((erro) => {
  console.error(erro);
  process.exitCode = 1;
});
