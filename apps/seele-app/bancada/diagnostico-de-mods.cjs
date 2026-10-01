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
    // **O retrato tem pendências próprias.** Soltá-lo sozinho é o que separa o
    // aviso do avatar do aviso dos selos, que chega pelo `dono.midiaMudou`.
    window.pendentesDoAvatar = [];
    testTable.ler_imagem_mod = () => new Promise((resolve) => pendentesDoAvatar.push(resolve));
    const a = instancia("mod/a");
    contribuicoesDosMods.registrar({ id: "mod/a" }, a, {
      ponto: "canal.item",
      modo: "adicionar",
      conteudo: [{ forma: "midia", chave: "selo", fonte: "img/selo.png" }],
    });
    window.avatarDeA = contribuicoesDosMods.registrar({ id: "mod/a" }, a, {
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
  // **O retrato avisa a gestão quando chega.** Ele chega sozinho, com os selos
  // ainda a caminho: soltos juntos, o aviso dos selos bastaria para a
  // conferência passar sem o do `.then` do avatar — e a recusa (R4e, em
  // `contribuicoes-e-camadas.cjs`) só exercita o `.catch`.
  await pagina.evaluate(() => {
    window.avisosDeMidia = 0;
    for (const pronto of pendentesDoAvatar.splice(0)) pronto({ papel: "imagem", uri: foto(), bytes: 64 });
  });
  await esperarQue(
    pagina,
    () => midiasDoPonto("pessoa.avatar").pronta === 1,
    "os bytes do retrato chegaram e ele não foi contado como «pronta»",
    contagens,
  );
  const quandoORetratoChegou = await pagina.evaluate(async () => {
    await new Promise((pronto) => setTimeout(pronto, 0));
    return { avisos: window.avisosDeMidia, selos: midiasDoPonto("canal.item").carregando };
  });
  assert.equal(
    quandoORetratoChegou.selos,
    2,
    "os selos chegaram junto com o retrato, e o aviso do retrato deixou de ser medido sozinho",
  );
  assert.ok(
    quandoORetratoChegou.avisos > 0,
    `o retrato chegou e a gestão não foi avisada (${quandoORetratoChegou.avisos} aviso(s))`,
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

  // **E avisa quando sai.** Revogar o avatar solta o retrato
  // (`soltarMontagem`): a contagem do ponto zera, e quem a mostra precisa
  // saber, ou a gestão continua dizendo «1 pronta» de um avatar que não existe.
  const quandoOAvatarSaiu = await pagina.evaluate(async () => {
    window.avisosDeMidia = 0;
    contribuicoesDosMods.revogar(avatarDeA.handle);
    await new Promise((pronto) => setTimeout(pronto, 0));
    return { avisos: window.avisosDeMidia, avatar: midiasDoPonto("pessoa.avatar") };
  });
  assert.deepEqual(
    quandoOAvatarSaiu.avatar,
    { carregando: 0, pronta: 0, recusada: 0, motivos: [] },
    "o avatar foi revogado e o retrato dele continuou contado em «pessoa.avatar»",
  );
  assert.ok(
    quandoOAvatarSaiu.avisos > 0,
    `o avatar foi revogado, o retrato saiu, e a gestão não foi avisada (${quandoOAvatarSaiu.avisos} aviso(s))`,
  );

  // **O motivo de um retrato recusado é cortado num lugar só**, em
  // `midiasDoPonto`, e por ponto de código, como as anotações da região: o
  // motivo do retrato vem do Rust ou de uma exceção, sem teto, e um par
  // substituto partido na posição 200 deixaria meio caractere na tela.
  const longo = `${"m".repeat(199)}😀${"n".repeat(100)}`;
  await pagina.evaluate((motivo) => {
    testTable.ler_imagem_mod = () => Promise.reject({ Recusado: { motivo } });
    contribuicoesDosMods.registrar({ id: "mod/a" }, modsCarregados.get("mod/a"), {
      ponto: "pessoa.avatar",
      modo: "substituir",
      alvo: "2",
      conteudo: { doServidor: { canal: 1, campo: "image", pedido: { transporte: "volume", path: "volume:outra" } } },
    });
  }, longo);
  await esperarQue(
    pagina,
    () => midiasDoPonto("pessoa.avatar").recusada === 1,
    "o retrato que o Rust recusou não foi contado como «recusada»",
    contagens,
  );
  const motivoDoRetrato = await pagina.evaluate(() => midiasDoPonto("pessoa.avatar").motivos.join(" | "));
  const pontosDoMotivo = [...motivoDoRetrato.replace(/^mod\/a: /, "")];
  assert.equal(
    motivoDoRetrato,
    `mod/a: ${"m".repeat(199)}😀`,
    "o motivo de um retrato recusado não saiu cortado em 200 pontos de código inteiros: "
    + `${pontosDoMotivo.length} pontos depois do MOD, o último U+${pontosDoMotivo.at(-1)?.codePointAt(0).toString(16).toUpperCase()}`
    + " (o certo: 200, o último U+1F600 — um corte por índice deixa meio par substituto)",
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

// ---------------------------------------------------------------------------
// «Quem pinta cada lugar», na gestão de MODs.
// ---------------------------------------------------------------------------

/** Espera a linha de um ponto em «quem pinta» dizer isto, e diz o que ela dizia se não disser. */
async function esperarALinha(pagina, ponto, padrao, oQueQuebra) {
  try {
    await pagina.waitForFunction(
      ([qual, fonte]) => new RegExp(fonte).test(
        document.querySelector(`#lista-quem-pinta li[data-ponto="${qual}"]`)?.textContent ?? "",
      ),
      [ponto, padrao.source],
      { timeout: 8000 },
    );
  } catch {
    const dizia = await pagina.evaluate(
      (qual) => document.querySelector(`#lista-quem-pinta li[data-ponto="${qual}"]`)?.textContent ?? "(sem linha)",
      ponto,
    );
    throw new Error(`${oQueQuebra} — a linha de «${ponto}» dizia: ${dizia}`);
  }
}

async function quemPintaCadaLugar(navegador, servidor) {
  const { pagina, erros } = await abrirASessao(navegador, servidor);
  await pagina.evaluate(() => {
    window.foto = () => {
      const tela = document.createElement("canvas");
      tela.width = tela.height = 8;
      return tela.toDataURL();
    };
    window.pendentes = [];
    testTable.midia_do_mod = (args) => (args.caminho === "img/falta.png"
      ? Promise.reject({ Recusado: { motivo: "arquivo-nao-declarado" } })
      : new Promise((resolve) => pendentes.push(resolve)));
    const a = instancia("mod/a");
    const b = instancia("mod/b");
    contribuicoesDosMods.registrar({ id: "mod/a" }, a, {
      ponto: "pessoa.cartao", modo: "substituir", prioridade: 10,
      conteudo: [{ forma: "texto", dentro: "cartão de A" }],
    });
    // O cartão de B tem mídia: quando B perde, ela continua montada — e não
    // pode ser dita como mídia de quem pinta.
    contribuicoesDosMods.registrar({ id: "mod/b" }, b, {
      ponto: "pessoa.cartao", modo: "substituir", prioridade: 5,
      conteudo: [{ forma: "texto", dentro: "cartão de B" }, { forma: "midia", chave: "rosto", fonte: "img/rosto.png" }],
    });
    contribuicoesDosMods.registrar({ id: "mod/a" }, a, {
      ponto: "canal.item", modo: "adicionar",
      conteudo: [{ forma: "midia", chave: "selo", fonte: "img/selo.png" }],
    });
    desenhar(testTable.snapshot);
    $("tela-server").hidden = false;
    $("painel-mods").hidden = false;
  });
  await pagina.click("#mods-aba-diagnostico");
  const linha = (ponto) => pagina.textContent(`#lista-quem-pinta li[data-ponto="${ponto}"]`);

  assert.equal(await pagina.locator("#lista-quem-pinta > li").count(), 11, "a aba não mostra os onze lugares");

  let cartao = await linha("pessoa.cartao");
  assert.match(cartao, /desenhado por mod\/a/, `sem preferência, quem vale é o de maior prioridade: ${cartao}`);
  assert.match(cartao, /pediram e não receberam: mod\/b/, `quem perdeu a disputa não foi dito: ${cartao}`);
  assert.match(cartao, /preferência desta máquina: automática/, `a preferência automática não foi dita: ${cartao}`);

  await pagina.evaluate(() => escolherApresentacao("pessoa.cartao", "mod/b"));
  cartao = await linha("pessoa.cartao");
  assert.match(cartao, /desenhado por mod\/b/, `com mod/b escolhido, ele não foi dito como quem desenha: ${cartao}`);
  assert.match(cartao, /pediram e não receberam: mod\/a/, `com mod/b escolhido, mod/a não foi dito como quem perdeu: ${cartao}`);
  assert.match(cartao, /preferência desta máquina: mod\/b/, `a escolha desta máquina não foi dita: ${cartao}`);
  await esperarALinha(pagina, "pessoa.cartao", /mídia: \d+ carregando$/, "o cartão de mod/b, escolhido, não montou a mídia dele");

  await pagina.evaluate(() => escolherApresentacao("pessoa.cartao", APRESENTACAO_NATIVA));
  cartao = await linha("pessoa.cartao");
  assert.match(cartao, /o SEELE desenha, por escolha desta máquina/, `o SEELE escolhido não foi dito como quem desenha: ${cartao}`);
  assert.match(cartao, /preferência desta máquina: o SEELE desenha/, `o valor reservado «o SEELE desenha» não foi dito: ${cartao}`);
  // **A mídia é de quem pinta.** O cartão de mod/b continua montado, com a
  // mídia a caminho, e quem pinta é o SEELE.
  assert.match(
    cartao,
    /mídia: nenhuma em uso agora$/,
    `a mídia do cartão de mod/b, que perdeu, foi dita ao lado de «o SEELE desenha»: ${cartao}`,
  );
  assert.match(await linha("pessoa.avatar"), /O cartão de cada pessoa/, "a preferência que o avatar herda do cartão não foi dita");

  // **A mídia acompanha a tela.** Dois canais, dois selos carregando; os bytes
  // chegam e a aba muda sozinha.
  await esperarALinha(pagina, "canal.item", /mídia: 2 carregando/, "os dois selos em voo não foram contados");
  await pagina.evaluate(() => {
    for (const pronto of pendentes.splice(0)) pronto({ papel: "imagem", uri: foto(), bytes: 64 });
  });
  await esperarALinha(pagina, "canal.item", /mídia: 2 prontas/, "os bytes chegaram e a aba não se redesenhou");

  // Uma recusa diz o motivo que o Rust deu, com o MOD na frente.
  await pagina.evaluate(() => {
    contribuicoesDosMods.registrar({ id: "mod/b" }, modsCarregados.get("mod/b"), {
      ponto: "canal.item", modo: "adicionar",
      conteudo: [{ forma: "midia", chave: "falta", fonte: "img/falta.png" }],
    });
    desenhar(testTable.snapshot);
  });
  await esperarALinha(
    pagina,
    "canal.item",
    /2 recusadas — mod\/b: arquivo-nao-declarado/,
    "a recusa não disse o motivo que o Rust deu, com o MOD na frente",
  );
  assert.match(await linha("canal.item"), /acrescentam: mod\/a, mod\/b/, "quem acrescenta a um lugar não foi dito");

  // Um lugar sem MOD fica na lista, e diz que está vazio.
  assert.match(await linha("compositor.ferramentas"), /nenhum MOD usa este lugar agora/, "um lugar sem MOD não disse que está vazio");

  // **O registro também redesenha a aba.** Um MOD que passa a usar um lugar
  // sem mídia nenhuma não dispara o aviso da mídia: só o `aoMudar` do registro
  // leva a mudança à tela.
  await pagina.evaluate(() => {
    contribuicoesDosMods.registrar({ id: "mod/a" }, modsCarregados.get("mod/a"), {
      ponto: "compositor.ferramentas", modo: "adicionar",
      conteudo: [{ forma: "texto", dentro: "ferramenta de A" }],
    });
  });
  await esperarALinha(
    pagina,
    "compositor.ferramentas",
    /acrescentam: mod\/a/,
    "um MOD passou a usar um lugar, sem mídia, e a aba não se redesenhou",
  );

  // **O caminho da API 3 também pinta.**
  await pagina.evaluate(() => {
    darCartoesDoMod({ id: "mod/b", hash: "mod-b" }, modsCarregados.get("mod/b"), { 2: [{ forma: "texto", dentro: "B" }] });
    escolherApresentacao("pessoa.cartao", "");
  });
  assert.match(await linha("pessoa.cartao"), /cartões pela API 3: mod\/b/, "um MOD que desenha cartões pela API 3 não apareceu em «quem pinta»");

  // E a mídia dele é de quem pinta enquanto ele vale: no automático, a foto do
  // cartão da API 3 conta (o cartão genérico de mod/b, que perdeu para mod/a,
  // não); com o SEELE escolhido, nenhuma das duas.
  await pagina.evaluate(() => {
    darCartoesDoMod({ id: "mod/b", hash: "mod-b" }, modsCarregados.get("mod/b"), {
      2: [{ forma: "texto", dentro: "B" }, { forma: "midia", chave: "foto", fonte: "img/b.png" }],
    });
  });
  await esperarALinha(
    pagina,
    "pessoa.cartao",
    /mídia: 1 carregando$/,
    "no automático, a foto do cartão da API 3 não foi contada, ou a mídia do cartão de mod/b, que perdeu, foi contada junto",
  );
  await pagina.evaluate(() => escolherApresentacao("pessoa.cartao", APRESENTACAO_NATIVA));
  cartao = await linha("pessoa.cartao");
  assert.match(
    cartao,
    /mídia: nenhuma em uso agora$/,
    `com o SEELE escolhido, a foto do cartão da API 3 de mod/b foi dita como mídia de quem pinta: ${cartao}`,
  );

  // **O caso parcial.** O avatar é por pessoa: mod/b desenha a Lia (2) e mod/a
  // o Alex (1) — os dois que o retrato da bancada mostra com quadrado de
  // avatar. No automático cada um desenha quem declarou, e os dois retratos
  // chegam.
  await pagina.evaluate(() => {
    escolherApresentacao("pessoa.cartao", "");
    testTable.ler_imagem_mod = () => Promise.resolve({ papel: "imagem", uri: foto(), bytes: 64 });
    const avatar = (alvo) => ({
      ponto: "pessoa.avatar", modo: "substituir", alvo,
      conteudo: { doServidor: { canal: 1, campo: "image", pedido: { transporte: "volume", path: `volume:${alvo}` } } },
    });
    contribuicoesDosMods.registrar({ id: "mod/b" }, modsCarregados.get("mod/b"), avatar("2"));
    contribuicoesDosMods.registrar({ id: "mod/a" }, modsCarregados.get("mod/a"), avatar("1"));
  });
  await esperarALinha(pagina, "pessoa.avatar", /mídia: 2 prontas$/, "os retratos de mod/b e de mod/a, cada um de quem o declarou, não chegaram");
  let avatar = await linha("pessoa.avatar");
  assert.match(
    avatar,
    /desenhado por mod\/b e mod\/a, cada um para quem declarou/,
    `no automático, cada MOD de avatar desenha quem declarou, e a linha não disse: ${avatar}`,
  );
  assert.doesNotMatch(
    avatar,
    /o SEELE desenha os outros/,
    `toda pessoa declarada tem quem a desenhe, e a linha disse que o SEELE desenha alguma: ${avatar}`,
  );

  // Com mod/b escolhido, o Alex — que só mod/a declarou — fica com o SEELE.
  // mod/b está de pé e desenha a Lia: nem «o SEELE desenha» do ponto inteiro,
  // nem «ele não está de pé». E o retrato de mod/a, que já tinha chegado,
  // continua montado sem pintar ninguém.
  await pagina.evaluate(() => escolherApresentacao("pessoa.avatar", "mod/b"));
  avatar = await linha("pessoa.avatar");
  assert.match(
    avatar,
    /desenhado por mod\/b, para quem declarou; o SEELE desenha os outros/,
    `com mod/b escolhido, o Alex (que só mod/a declarou) é desenhado pelo SEELE, e a linha não disse: ${avatar}`,
  );
  assert.match(avatar, /pediram e não receberam: mod\/a/, `com mod/b escolhido, mod/a não foi dito como quem perdeu: ${avatar}`);
  assert.doesNotMatch(avatar, /não está de pé/, `mod/b desenha a Lia e foi dito como quem não está de pé: ${avatar}`);
  assert.match(
    avatar,
    /mídia: 1 pronta$/,
    `o retrato de mod/a, que perdeu, foi contado como mídia de quem pinta: ${avatar}`,
  );

  assert.deepEqual(erros, [], `a página lançou erro durante «quem pinta cada lugar»: ${erros.join(" | ")}`);
  await pagina.close();
}

const PROVAS = [
  aMidiaDeCadaPontoEContadaEDita,
  quemPintaCadaLugar,
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
