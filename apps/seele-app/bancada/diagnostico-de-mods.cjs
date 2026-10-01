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
//   canal de baixo, não tiram o foco de quem escreve, e contornam só o que a
//   rolagem deixa à vista;
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
 * @param {*} [argumento] O que `condicao` recebe na página.
 */
async function esperarQue(pagina, condicao, oQueQuebra, comoFicou, argumento = null) {
  try {
    await pagina.waitForFunction(condicao, argumento, { timeout: 10_000 });
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

// ---------------------------------------------------------------------------
// «Quem desenha o que», na gestão de MODs.
// ---------------------------------------------------------------------------

/** O texto e os botões da linha de um ponto em «quem desenha o que». */
function linhaDaGestao(pagina, ponto) {
  return pagina.evaluate((nome) => {
    desenharApresentacoes();
    const item = [...document.querySelectorAll("#lista-apresentacoes > li")]
      .find((li) => li.querySelector(".mods-id")?.textContent === nome);
    return {
      estado: item?.querySelector(".mods-versao")?.textContent ?? "(sem linha)",
      botoes: [...(item?.querySelectorAll("button") ?? [])].map((b) => b.textContent),
    };
  }, ponto);
}

async function quemDesenhaOQueDizAEscolhaDestaMaquina(navegador, servidor) {
  const { pagina, erros } = await abrirASessao(navegador, servidor);
  await pagina.evaluate(() => {
    const a = instancia("mod/a");
    const b = instancia("mod/b");
    contribuicoesDosMods.registrar({ id: "mod/a" }, a, {
      ponto: "pessoa.cartao", modo: "substituir", prioridade: 10, conteudo: [{ forma: "texto", dentro: "A" }],
    });
    contribuicoesDosMods.registrar({ id: "mod/b" }, b, {
      ponto: "pessoa.cartao", modo: "substituir", prioridade: 5, conteudo: [{ forma: "texto", dentro: "B" }],
    });
  });
  const cartao = "O cartão de cada pessoa";

  // **A escolha desta máquina é a deste servidor.** A preferência é gravada
  // com o destino na chave (`chaveDaPreferencia`), e a gestão a lia pelo ponto
  // só: dizia «(escolha automática)» de qualquer escolha, e o botão de voltar
  // ao automático nunca aparecia.
  let linha = await linhaDaGestao(pagina, cartao);
  assert.equal(
    linha.estado,
    "apresentado por mod/a (escolha automática)",
    `sem escolha, quem vale é o de maior prioridade, e a gestão disse outra coisa: ${linha.estado}`,
  );
  assert.ok(
    !linha.botoes.includes("DECIDIR AUTOMATICAMENTE"),
    `no automático, a gestão ofereceu voltar ao automático: ${linha.botoes.join(", ")}`,
  );

  await pagina.evaluate(() => escolherApresentacao("pessoa.cartao", "mod/b"));
  linha = await linhaDaGestao(pagina, cartao);
  assert.equal(
    linha.estado,
    "apresentado por mod/b",
    `com mod/b escolhido nesta máquina, a gestão não disse que ele apresenta por escolha: ${linha.estado}`,
  );
  assert.ok(
    linha.botoes.includes("DECIDIR AUTOMATICAMENTE") && linha.botoes.includes("USAR mod/a"),
    `com mod/b escolhido, a gestão não ofereceu voltar ao automático nem trocar para mod/a: ${linha.botoes.join(", ")}`,
  );

  await pagina.evaluate(() => escolherApresentacao("pessoa.cartao", APRESENTACAO_NATIVA));
  linha = await linhaDaGestao(pagina, cartao);
  assert.equal(
    linha.estado,
    "o SEELE desenha; nenhum MOD substitui este lugar",
    `com o SEELE escolhido nesta máquina, a gestão não disse que o SEELE desenha: ${linha.estado}`,
  );
  assert.ok(
    linha.botoes.includes("DECIDIR AUTOMATICAMENTE") && !linha.botoes.includes("USAR APRESENTAÇÃO DO SEELE"),
    `com o SEELE escolhido, a gestão não ofereceu voltar ao automático, ou ofereceu o que já vale: ${linha.botoes.join(", ")}`,
  );
  await pagina.evaluate(() => escolherApresentacao("pessoa.cartao", ""));

  assert.deepEqual(erros, [], `a página lançou erro em «quem desenha o que»: ${erros.join(" | ")}`);
  await pagina.close();
}

// ---------------------------------------------------------------------------
// O modo de desenvolvedor contorna sem tomar nada.
// ---------------------------------------------------------------------------

async function oModoDeDesenvolvedorContornaSemTomarNada(navegador, servidor) {
  const { pagina, erros } = await abrirASessao(navegador, servidor);
  await pagina.evaluate(() => {
    // Um MOD que acrescenta ao item de canal: o contorno cerca o contêiner com
    // o conteúdo do MOD dentro, e o clique tem de atravessar os dois.
    contribuicoesDosMods.registrar({ id: "mod/a" }, instancia("mod/a"), {
      ponto: "canal.item",
      modo: "adicionar",
      conteudo: [{ forma: "texto", dentro: "A" }],
    });
    desenhar(testTable.snapshot);
  });
  assert.equal(
    await pagina.evaluate(() => $("contornos-dos-pontos").hidden),
    true,
    "os contornos apareceram sem ninguém ligar o modo",
  );

  // Liga pela configuração, como quem usa liga.
  await pagina.evaluate(() => {
    $("tela-server").hidden = false;
    $("painel-mods").hidden = false;
  });
  await pagina.click("#mods-aba-diagnostico");
  await pagina.click("#mods-modo-desenvolvedor");
  await pagina.evaluate(() => {
    $("tela-server").hidden = true;
  });
  await esperarQue(
    pagina,
    () => document.querySelectorAll('.contorno-de-ponto[data-ponto="canal.item"]').length === 2,
    "ligar o modo pela configuração não contornou os dois canais",
    () => [...document.querySelectorAll(".contorno-de-ponto")].map((c) => c.dataset.ponto),
  );
  const vistos = await pagina.evaluate(() => [
    ...new Set([...document.querySelectorAll(".contorno-de-ponto")].map((c) => c.dataset.ponto)),
  ]);
  for (const ponto of [
    "canal.cabecalho", "canal.item", "compositor.ferramentas", "pessoa.acoes", "pessoa.avatar",
    "pessoa.cartao", "pessoa.identidade", "sala.acoes", "servidor.aparencia",
  ]) {
    assert.ok(vistos.includes(ponto), `o contorno de «${ponto}» não apareceu; apareceram: ${vistos.join(", ")}`);
  }
  assert.equal(
    await pagina.evaluate(() => localStorage.getItem("seele.mods.desenvolvedor")),
    "sim",
    "ligar o modo não ficou guardado nesta máquina",
  );

  // **Nada na camada é focável.** O `aria-hidden` tira os contornos do leitor
  // de tela, e não da tabulação: um botão ali dentro seria uma parada de Tab
  // invisível, e com o foco nele a barra de espaço deixa de falar.
  const focaveis = await pagina.evaluate(() => {
    const camada = $("contornos-dos-pontos");
    return [camada, ...camada.querySelectorAll("*")]
      .filter((no) => no.tabIndex >= 0)
      .map((no) => `${no.tagName.toLowerCase()}.${no.className}`);
  });
  assert.deepEqual(
    focaveis,
    [],
    `a camada dos contornos ganhou algo focável: ${focaveis.join(", ")}`,
  );

  // **O foco fica com quem escreve**, mesmo com a página mudando embaixo.
  await pagina.focus("#campo-mensagem");
  await pagina.evaluate(() => desenhar(testTable.snapshot));
  await pagina.evaluate(() => new Promise((pronto) => requestAnimationFrame(() => requestAnimationFrame(pronto))));
  assert.equal(
    await pagina.evaluate(() => document.activeElement?.id),
    "campo-mensagem",
    "redesenhar os contornos tirou o foco do campo de escrever — e com ele a barra de espaço, que fala",
  );

  // **O clique atravessa.** O meio do canal 2 é do canal, e apertar ali o abre.
  const alvo = await pagina.evaluate(() => {
    const botao = document.querySelector('#lista-linhas button[data-linha="2"]');
    const caixa = botao.getBoundingClientRect();
    const x = caixa.left + caixa.width / 2;
    const y = caixa.top + caixa.height / 2;
    const achado = document.elementFromPoint(x, y);
    return {
      x,
      y,
      de: achado?.closest?.('button[data-linha="2"]') ? "canal" : String(achado?.className ?? achado),
    };
  });
  assert.equal(alvo.de, "canal", `o meio do canal é de «${alvo.de}»: o contorno está pegando o clique`);
  await pagina.mouse.click(alvo.x, alvo.y);
  await esperarQue(
    pagina,
    () => testCalls.some((c) => c.cmd === "open_channel" && c.args?.channel === 2),
    "o clique no meio do canal 2, com o contorno por cima, não abriu o canal",
  );

  // **Guardado nesta máquina**: a janela reaberta volta com o modo ligado.
  await pagina.reload();
  await prepararASessao(pagina);
  assert.equal(
    await pagina.evaluate(() => $("mods-modo-desenvolvedor").checked),
    true,
    "a marca da configuração voltou desmarcada com o modo guardado como ligado",
  );
  await esperarQue(
    pagina,
    () => document.querySelectorAll(".contorno-de-ponto").length > 0,
    "a janela reaberta, com o modo guardado como ligado, não desenhou contorno nenhum",
  );

  // **O contorno é do que se vê.** Uma conversa longa, rolada até o fim: os
  // retratos que a rolagem levou para cima da lista continuam com caixa na
  // janela — sobre o cabeçalho do canal —, e só o recorte da lista os
  // esconde. Sem o recorte, o contorno de um retrato que ninguém vê fica
  // desenhado por cima do nome do canal.
  const rolada = await pagina.evaluate(async () => {
    const agora = Math.floor(Date.now() / 1000);
    const longa = Array.from({ length: 80 }, (_, i) => ({
      id: 100 + i,
      channel: 1,
      author: i % 2 ? 2 : 1,
      author_nickname: i % 2 ? "Lia" : "Alex",
      at_seconds: agora - (80 - i) * 900,
      body: `mensagem ${i}, com texto bastante para ocupar a linha`,
      own: i % 2 === 0,
      edited: false,
      attachment: null,
    }));
    // A mesma conversa para quem a buscar de novo: uma busca em voo não a
    // troca pela curta no meio da medida.
    testTable.messages = longa;
    mensagens = longa;
    desenharMensagens();
    const lista = $("lista-mensagens");
    lista.scrollTop = lista.scrollHeight;
    for (let volta = 0; volta < 2; volta += 1) {
      await new Promise((pronto) => requestAnimationFrame(() => requestAnimationFrame(pronto)));
    }
    const area = lista.getBoundingClientRect();
    const naJanela = (r) => r.width + r.height > 0 && r.bottom > 0 && r.top < innerHeight;
    const cortados = [...lista.querySelectorAll("[data-pessoa-do-avatar]")]
      .map((no) => no.getBoundingClientRect())
      .filter((r) => naJanela(r) && (r.top < area.top || r.bottom > area.bottom));
    // Os contornos de retrato na faixa da lista: os da conversa, e não os da
    // lista de pessoas, que mora em outra coluna.
    const daLista = [...document.querySelectorAll('.contorno-de-ponto[data-ponto="pessoa.avatar"]')]
      .map((c) => {
        const left = parseFloat(c.style.left);
        const top = parseFloat(c.style.top);
        return { left, top, right: left + parseFloat(c.style.width), bottom: top + parseFloat(c.style.height) };
      })
      .filter((c) => c.left >= area.left - 1 && c.right <= area.right + 1);
    const fora = daLista.filter((c) => c.top < area.top - 1 || c.bottom > area.bottom + 1);
    return {
      rolou: lista.scrollTop,
      area: `${Math.round(area.top)}–${Math.round(area.bottom)}px`,
      cortados: cortados.length,
      vistos: daLista.length - fora.length,
      fora: fora.map((c) => `${Math.round(c.top)}–${Math.round(c.bottom)}px`),
    };
  });
  assert.ok(rolada.rolou > 0, "a conversa longa não rolou: a prova do recorte não tem o que recortar");
  assert.ok(
    rolada.cortados > 0,
    `a lista rolada não deixou nenhum retrato cortado com caixa na janela (lista em ${rolada.area}): a prova do recorte não prova nada`,
  );
  assert.ok(
    rolada.vistos > 0,
    `nenhum retrato à vista na lista rolada ganhou contorno (lista em ${rolada.area}): o recorte apagou o que se vê`,
  );
  assert.deepEqual(
    rolada.fora,
    [],
    `com a lista rolada, contornos de retrato saíram dela (a lista vai de ${rolada.area}; fora dela: `
    + `${rolada.fora.join(", ")}): o contorno de um lugar que a rolagem escondeu fica desenhado por cima do `
    + "cabeçalho do canal",
  );

  // E desligar tira tudo, e esquece.
  await pagina.evaluate(() => {
    $("tela-server").hidden = false;
    $("painel-mods").hidden = false;
  });
  await pagina.click("#mods-aba-diagnostico");
  await pagina.click("#mods-modo-desenvolvedor");
  const desligado = await pagina.evaluate(() => ({
    oculta: $("contornos-dos-pontos").hidden,
    filhos: $("contornos-dos-pontos").childElementCount,
    guardado: localStorage.getItem("seele.mods.desenvolvedor"),
  }));
  assert.deepEqual(
    desligado,
    { oculta: true, filhos: 0, guardado: null },
    "desligar o modo deixou contorno na tela ou a escolha guardada",
  );
  assert.deepEqual(erros, [], `a página lançou erro durante o modo de desenvolvedor: ${erros.join(" | ")}`);
  await pagina.close();
}

// ---------------------------------------------------------------------------
// O som de MOD toca por WebAudio, sob a CSP do produto.
// ---------------------------------------------------------------------------

/** Um WAV de 440 Hz — por padrão de um segundo e meio —, feito aqui: nenhum binário no repositório. */
function wavDeTeste(segundos = 1.5) {
  const taxa = 8000;
  const amostras = Math.round(taxa * segundos);
  const bytes = Buffer.alloc(44 + amostras * 2);
  bytes.write("RIFF", 0, "ascii");
  bytes.writeUInt32LE(36 + amostras * 2, 4);
  bytes.write("WAVE", 8, "ascii");
  bytes.write("fmt ", 12, "ascii");
  bytes.writeUInt32LE(16, 16);
  bytes.writeUInt16LE(1, 20); // PCM
  bytes.writeUInt16LE(1, 22); // mono
  bytes.writeUInt32LE(taxa, 24);
  bytes.writeUInt32LE(taxa * 2, 28);
  bytes.writeUInt16LE(2, 32);
  bytes.writeUInt16LE(16, 34);
  bytes.write("data", 36, "ascii");
  bytes.writeUInt32LE(amostras * 2, 40);
  for (let i = 0; i < amostras; i += 1) {
    bytes.writeInt16LE(Math.round(Math.sin((2 * Math.PI * 440 * i) / taxa) * 8000), 44 + i * 2);
  }
  return [...bytes];
}

/** Espera o MOD receber este estado da mídia `sino`, e diz o que veio se não vier. */
async function esperarOSino(pagina, estado, vezes = 1) {
  try {
    await pagina.waitForFunction(
      ([quer, quantas]) => eventosDoMod
        .filter((e) => e.nome === "midia" && e.chave === "sino" && e.estado === quer)
        .length >= quantas,
      [estado, vezes],
      { timeout: 8000 },
    );
  } catch {
    const vieram = await pagina.evaluate(() => eventosDoMod
      .filter((e) => e.nome === "midia")
      .map((e) => `${e.estado}${e.porque ? ` (${e.porque})` : ""}`));
    throw new Error(`o MOD esperava «${estado}» do som e recebeu: ${vieram.join(", ") || "nada"}`);
  }
}

async function oSomDeModTocaPorWebAudioSobACspDoProduto(navegador, servidor) {
  const { pagina, erros, recusasDaCsp } = await abrirASessao(navegador, servidor);
  const declarar = (tocando) => pagina.evaluate((quer) => {
    desenharARegiaoDoMod({ id: "mod/a", hash: "mod-a" }, somA, [
      { forma: "midia", chave: "sino", fonte: "som/sino.wav", descricao: "Sino de teste", tocando: quer },
    ]);
  }, tocando);
  await pagina.evaluate((bytes) => {
    // Quantas fontes de som foram paradas: é o que prova que sair no meio do
    // som para o som, e não só o esconde.
    window.paradas = 0;
    const parar = AudioBufferSourceNode.prototype.stop;
    AudioBufferSourceNode.prototype.stop = function (...args) {
      window.paradas += 1;
      return parar.apply(this, args);
    };
    testTable.midia_do_mod = { papel: "som", uri: "data:audio/wav;base64,UklGRg==", bytes: bytes.length };
    // `som_do_mod` como ele chega pelo protocolo `ipc:`: um `ArrayBuffer` novo
    // a cada pedido. O `number[]` do `postMessage` é o que a bancada em `vm`
    // (`regiao-do-mod.cjs`) entrega; as duas formas ficam medidas.
    testTable.som_do_mod = () => new Uint8Array(bytes).buffer;
    window.somA = instancia("mod/a");
  }, wavDeTeste());
  await declarar(false);

  // Os bytes e a decodificação são duas voltas: espera-se a figura sair de
  // «carregando», e o que ela disse é o que se confere.
  await pagina.waitForFunction(() => {
    const estado = document.querySelector('#regioes-dos-mods figure[data-chave-do-mod="sino"]')?.dataset.estado;
    return Boolean(estado) && estado !== "carregando";
  }, null, { timeout: 8000 });
  const montado = await pagina.evaluate(() => ({
    porque: eventosDoMod.find((e) => e.nome === "midia" && e.estado === "falhou")?.porque ?? "",
    audios: document.querySelectorAll("audio").length,
    botao: document.querySelector("#regioes-dos-mods .regiao-de-mod-som")?.textContent ?? null,
    figura: document.querySelector('#regioes-dos-mods figure[data-chave-do-mod="sino"]')?.dataset.estado ?? null,
    pedido: testCalls.find((c) => c.cmd === "som_do_mod")?.args ?? null,
    eventos: eventosDoMod.filter((e) => e.nome === "midia").length,
  }));
  assert.equal(montado.audios, 0, "um elemento <audio> voltou à página, e a CSP desta janela recusa o data: dele");
  assert.equal(montado.figura, "pronta", `o som não chegou a ficar pronto: ${montado.figura} (${montado.porque})`);
  assert.equal(montado.botao, "TOCAR", `o botão do produto não começou em TOCAR: ${montado.botao}`);
  assert.equal(montado.eventos, 0, "o som tocou sem que o MOD declarasse e sem que alguém apertasse");
  assert.deepEqual(
    montado.pedido,
    { geracao: 1, id: "mod/a", hash: "mod-a", caminho: "som/sino.wav" },
    "os bytes do som não foram pedidos ao Rust pelo contrato de `som_do_mod`: a geração de pé, "
    + "o id e o hash do MOD e o caminho declarado",
  );

  // Quem aperta o botão do produto ouve, e o fim é dito ao MOD.
  await pagina.click("#regioes-dos-mods .regiao-de-mod-som");
  await esperarOSino(pagina, "tocando");
  assert.equal(
    await pagina.textContent("#regioes-dos-mods .regiao-de-mod-som"),
    "PAUSAR",
    "o som começou e o botão do produto não passou a PAUSAR",
  );
  await esperarOSino(pagina, "terminou");
  assert.equal(
    await pagina.textContent("#regioes-dos-mods .regiao-de-mod-som"),
    "TOCAR",
    "o som terminou e o botão do produto ficou em PAUSAR",
  );

  // A declaração do MOD toca **na mudança**; e sair no meio do som para o som.
  await declarar(true);
  await esperarOSino(pagina, "tocando", 2);
  const antes = await pagina.evaluate(() => window.paradas);
  await pagina.evaluate(() => limparARegiaoDoMod("mod/a", somA));
  assert.equal(
    await pagina.evaluate(() => window.paradas),
    antes + 1,
    "sair no meio do som não parou a fonte, e o som continua tocando sem dono",
  );
  assert.equal(await pagina.locator(".regiao-de-mod-som").count(), 0, "o botão do som ficou na tela depois de a região sair");

  // **E numa contribuição.** Ela é montada uma vez por destino — um
  // `canal.item` sem alvo, num retrato de dois canais, monta duas vezes —, e
  // cada montagem pede os bytes com o mesmo contrato: sem o `hash`, o Tauri
  // recusa o pedido («missing required key hash») e o som nunca chega ao
  // WebAudio. Conferido em **todo** pedido de mídia da página, e não no
  // primeiro: o primeiro é o da região, que já tinha o hash.
  //
  // E o som mora **num destino só**: um pede os bytes, decodifica e toca, e
  // o outro mostra a figura sem tocador (`montarSom`). Por destino, dois
  // canais eram dois sinos ao mesmo tempo, o MOD ouvia «tocando» duas vezes de
  // um som que declarou uma, e cada canal segurava a sua cópia decodificada —
  // numa sala de 64 pessoas, 64.
  const pedidosAntes = await pagina.evaluate(() => testCalls.length);
  await pagina.evaluate(() => {
    contribuicoesDosMods.registrar({ id: "mod/a" }, somA, {
      ponto: "canal.item",
      modo: "adicionar",
      conteudo: [{ forma: "midia", chave: "sino-do-canal", fonte: "som/sino.wav", tocando: true }],
    });
    desenhar(testTable.snapshot);
  });
  await esperarQue(
    pagina,
    () => {
      const figuras = [...document.querySelectorAll('.contribuicao-de-mod figure[data-chave-do-mod="sino-do-canal"]')];
      return figuras.length === 2 && figuras.every((f) => f.dataset.estado && f.dataset.estado !== "carregando");
    },
    "o som da contribuição sem alvo não assentou nos dois canais",
    () => [...document.querySelectorAll('.contribuicao-de-mod figure[data-chave-do-mod="sino-do-canal"]')]
      .map((f) => f.dataset.estado),
  );
  await pagina.evaluate(() => new Promise((pronto) => requestAnimationFrame(() => requestAnimationFrame(pronto))));
  const daContribuicao = await pagina.evaluate((desde) => ({
    figuras: [...document.querySelectorAll('.contribuicao-de-mod figure[data-chave-do-mod="sino-do-canal"]')]
      .map((f) => f.dataset.estado),
    eventos: eventosDoMod
      .filter((e) => e.nome === "midia" && e.chave === "sino-do-canal")
      .map((e) => `${e.estado}${e.porque ? ` (${e.porque})` : ""}`),
    semHash: testCalls
      .filter((c) => c.cmd === "midia_do_mod" || c.cmd === "som_do_mod")
      .filter((c) => c.args?.hash !== "mod-a")
      .map((c) => `${c.cmd} ${JSON.stringify(c.args)}`),
    bytesPedidos: testCalls.slice(desde).filter((c) => c.cmd === "som_do_mod").length,
  }), pedidosAntes);
  assert.deepEqual(
    daContribuicao.figuras,
    ["pronta", "pronta"],
    `o som da contribuição sem alvo não assentou nos dois canais: ${daContribuicao.figuras.join(", ")}`,
  );
  assert.equal(
    daContribuicao.bytesPedidos,
    1,
    `os bytes do som da contribuição sem alvo foram pedidos ${daContribuicao.bytesPedidos} vez(es), e não `
    + "uma — a mais é um canal a mais decodificando e segurando a sua cópia; nenhuma, um som que nunca toca",
  );
  assert.deepEqual(
    daContribuicao.semHash,
    [],
    "a mídia de uma contribuição foi pedida sem o hash do MOD, e o Tauri recusa o pedido — "
    + `o som de contribuição nunca chega ao WebAudio: ${daContribuicao.semHash.join(" | ")}`,
  );
  assert.deepEqual(
    daContribuicao.eventos.filter((e) => e === "tocando"),
    ["tocando"],
    "o som da contribuição sem alvo tocou uma vez por canal, todos juntos, e não uma vez: "
    + `${daContribuicao.eventos.join(", ") || "nada"}`,
  );

  // Nenhuma mídia passou pela CSP: os bytes foram para o WebAudio.
  assert.deepEqual(recusasDaCsp, [], `a CSP recusou mídia: ${recusasDaCsp.join(" | ")}`);
  assert.deepEqual(erros, [], `a página lançou erro durante o som de MOD: ${erros.join(" | ")}`);
  await pagina.close();
}

// ---------------------------------------------------------------------------
// O som que sai da tela para, como o `<audio>` parava.
// ---------------------------------------------------------------------------

/**
 * Os caminhos em que o produto tira um nó de MOD da tela **sem** descartá-lo,
 * no Chromium, com o WebAudio de verdade (I-1 da revisão ampla do Plano 1D,
 * medido na sonda «som solto»): a página fechada pela saída do produto, o som
 * novo que uma página fechada declara `tocando`, a contribuição que perde a
 * disputa para «o SEELE desenha», o cartão da API 3 que deixa a lista, e o
 * destino de quem saiu do servidor.
 *
 * Antes, nos cinco, a fonte seguia até o fim fora da tela, e o MOD não ouvia
 * `pausada`. Um `<audio>` tirado do documento pausava sozinho.
 */
async function oSomQueSaiDaTelaPara(navegador, servidor) {
  const { pagina, erros, recusasDaCsp } = await abrirASessao(navegador, servidor);
  await pagina.evaluate((bytes) => {
    // Quantas fontes pararam, e quantas estão tocando agora.
    window.paradas = 0;
    window.ativas = 0;
    const parar = AudioBufferSourceNode.prototype.stop;
    AudioBufferSourceNode.prototype.stop = function (...args) {
      window.paradas += 1;
      return parar.apply(this, args);
    };
    const comecar = AudioBufferSourceNode.prototype.start;
    AudioBufferSourceNode.prototype.start = function (...args) {
      window.ativas += 1;
      this.addEventListener("ended", () => {
        window.ativas -= 1;
      });
      return comecar.apply(this, args);
    };
    testTable.midia_do_mod = { papel: "som", uri: "data:audio/wav;base64,UklGRg==", bytes: bytes.length };
    testTable.som_do_mod = () => new Uint8Array(bytes).buffer;
    window.somA = instancia("mod/a");
    window.somB = instancia("mod/b");
  }, wavDeTeste(6));
  const doSom = (chave) => pagina.evaluate((c) => eventosDoMod
    .filter((e) => e.nome === "midia" && e.chave === c)
    .map((e) => `${e.estado}${e.porque ? ` (${e.porque})` : ""}`), chave);
  const esperarOSom = (chave, estado, oQueQuebra) => esperarQue(
    pagina,
    ([c, quer]) => eventosDoMod.some((e) => e.nome === "midia" && e.chave === c && e.estado === quer),
    oQueQuebra,
    // O `comoFicou` roda na página, sem os argumentos: lê o que o MOD ouviu.
    () => eventosDoMod.filter((e) => e.nome === "midia").map((e) => `${e.chave}: ${e.estado}`),
    [chave, estado],
  );
  const contagem = () => pagina.evaluate(() => ({ paradas: window.paradas, ativas: window.ativas }));

  // 1. A página fechada pela saída do produto, com a trilha tocando.
  await pagina.evaluate(() => {
    const conjunto = superficiesDoMod({ id: "mod/a", hash: "mod-a" }, somA);
    conjunto.criar({ id: "mesa", tipo: "pagina", titulo: "Mesa" });
    conjunto.de("mesa").montar([{ forma: "midia", chave: "trilha", fonte: "som/trilha.wav", descricao: "Trilha" }]);
  });
  await esperarQue(
    pagina,
    () => document.querySelector('figure[data-chave-do-mod="trilha"] .regiao-de-mod-som'),
    "a trilha da página não ficou de pé, com o botão do produto",
  );
  await pagina.click('figure[data-chave-do-mod="trilha"] .regiao-de-mod-som');
  await esperarOSom("trilha", "tocando", "o botão do produto não tocou a trilha da página");
  const antesDeFechar = await contagem();
  await pagina.evaluate(() => superficiesDoMod({ id: "mod/a", hash: "mod-a" }, somA).de("mesa").pedirFechamento("saida-do-produto"));
  await esperarOSom(
    "trilha",
    "pausada",
    "a página fechou pela saída do produto e o MOD não ouviu «pausada»: a trilha segue tocando fora da tela",
  );
  await esperarQue(pagina, () => window.ativas === 0, "a página fechou e a fonte da trilha continuou tocando", () => window.ativas);
  const depoisDeFechar = await contagem();
  assert.equal(
    depoisDeFechar.paradas,
    antesDeFechar.paradas + 1,
    "a página fechou e a fonte da trilha não foi parada: o som segue até o fim, sem controle à vista",
  );

  // 2. A página fechada recebe um som novo declarado `tocando`: ele não toca,
  // e a recusa, com o motivo, vai ao MOD e ao registro.
  await pagina.evaluate(() => superficiesDoMod({ id: "mod/a", hash: "mod-a" }, somA).de("mesa").montar([
    { forma: "midia", chave: "trilha", fonte: "som/trilha.wav", descricao: "Trilha" },
    { forma: "midia", chave: "combate", fonte: "som/combate.wav", tocando: true },
  ]));
  await esperarOSom(
    "combate",
    "recusada",
    "a página fechada recebeu um som novo declarado `tocando`, e ele não foi recusado",
  );
  const recusa = await doSom("combate");
  assert.ok(
    recusa.some((e) => /tela/.test(e)) && !recusa.some((e) => e.startsWith("tocando")),
    `o som novo da página fechada tocou, ou foi recusado sem dizer por quê ao MOD: ${recusa.join(", ")}`,
  );
  const linha = await pagina.evaluate(() => testCalls
    .filter((c) => c.cmd === "registrar_da_janela" && String(c.args?.oQue ?? "").includes("«combate»"))
    .map((c) => c.args.oQue));
  assert.ok(
    linha.some((texto) => /tela/.test(texto)),
    `a recusa do som da página fechada não chegou ao registro: ${JSON.stringify(linha)}`,
  );
  assert.equal((await contagem()).ativas, 0, "um som começou a tocar dentro da página fechada");

  // 3. A contribuição que perde a disputa para «o SEELE desenha».
  await pagina.evaluate(() => {
    contribuicoesDosMods.registrar({ id: "mod/a" }, somA, {
      ponto: "pessoa.cartao",
      modo: "substituir",
      alvo: "2",
      conteudo: [{ forma: "texto", dentro: "Lia (mod/a)" }, { forma: "midia", chave: "vinheta", fonte: "som/v.wav", tocando: true }],
    });
    desenhar(testTable.snapshot);
  });
  await esperarOSom("vinheta", "tocando", "a vinheta da substituição de `pessoa.cartao` não tocou");
  // **Sem o retrato periódico, daqui até o fim do caso 4b** (S-m2 da revisão
  // do Lote Som). A página roda `atualizar()` a cada meio segundo, e cada volta
  // é um `desenhar` que também varre: com ele de pé, tirar a varredura de
  // `escolherApresentacao` ou a do `aoMudar` deixava esta prova verde. Parado,
  // o que cala o som é o desenho que a escolha ou o registro fazem na hora.
  await pagina.evaluate(() => {
    window.retratoDaSessao = testTable.snapshot;
    testTable.snapshot = () => Promise.reject("NotConnected");
  });
  const antesDoNativo = await contagem();
  await pagina.evaluate(() => escolherApresentacao("pessoa.cartao", APRESENTACAO_NATIVA));
  await esperarOSom(
    "vinheta",
    "pausada",
    "«o SEELE desenha» o cartão, a substituição saiu da tela, e o MOD não ouviu «pausada»: a vinheta segue tocando",
  );
  assert.equal(
    (await contagem()).paradas,
    antesDoNativo.paradas + 1,
    "«o SEELE desenha» o cartão e a fonte da vinheta não foi parada: o som de quem perdeu a disputa segue até o fim",
  );
  await pagina.evaluate(() => {
    escolherApresentacao("pessoa.cartao", "");
    contribuicoesDosMods.revogarDoMod("mod/a");
  });

  // 4. O cartão da API 3 sob «o SEELE desenha».
  await pagina.evaluate(() => darCartoesDoMod({ id: "mod/b", hash: "mod-b" }, somB, {
    2: [{ forma: "midia", chave: "cartao-de-b", fonte: "som/b.wav", tocando: true }],
  }));
  await esperarOSom("cartao-de-b", "tocando", "o som do cartão da API 3 não tocou");
  const antesDoCartao = await contagem();
  await pagina.evaluate(() => escolherApresentacao("pessoa.cartao", APRESENTACAO_NATIVA));
  await esperarOSom(
    "cartao-de-b",
    "pausada",
    "«o SEELE desenha» o cartão, o cartão da API 3 saiu da lista, e o MOD não ouviu «pausada»",
  );
  assert.equal(
    (await contagem()).paradas,
    antesDoCartao.paradas + 1,
    "o cartão da API 3 saiu da lista e a fonte dele não foi parada",
  );
  await pagina.evaluate(() => escolherApresentacao("pessoa.cartao", ""));

  // 4b. A substituição que perde para outra, registrada depois: quem tira o
  // nó da lista é o `aoMudar` do registro, e não uma escolha nem um retrato.
  await pagina.evaluate(() => contribuicoesDosMods.registrar({ id: "mod/a" }, somA, {
    ponto: "pessoa.cartao",
    modo: "substituir",
    alvo: "2",
    prioridade: 0,
    conteudo: [{ forma: "texto", dentro: "Lia (mod/a)" }, { forma: "midia", chave: "abertura", fonte: "som/ab.wav", tocando: true }],
  }));
  await esperarOSom("abertura", "tocando", "a abertura da substituição de `pessoa.cartao` não tocou");
  const antesDaOutra = await contagem();
  await pagina.evaluate(() => contribuicoesDosMods.registrar({ id: "mod/b" }, somB, {
    ponto: "pessoa.cartao",
    modo: "substituir",
    alvo: "2",
    prioridade: 10,
    conteudo: [{ forma: "texto", dentro: "Lia (mod/b)" }],
  }));
  await esperarOSom(
    "abertura",
    "pausada",
    "outra substituição tomou o cartão da Lia, a de mod/a saiu da lista, e o MOD não ouviu «pausada»",
  );
  assert.equal(
    (await contagem()).paradas,
    antesDaOutra.paradas + 1,
    "outra substituição tomou o cartão da Lia e a fonte da abertura de mod/a não foi parada",
  );
  await pagina.evaluate(() => {
    contribuicoesDosMods.revogarDoMod("mod/a");
    contribuicoesDosMods.revogarDoMod("mod/b");
    testTable.snapshot = window.retratoDaSessao;
  });

  // 5. Quem sai: a contribuição continua montada para o destino de quem saiu
  // do servidor (`montadas` não encolhe), e o nó sai do documento no retrato
  // seguinte — que é só um `desenhar`, sem escolha nem registro mudando.
  await pagina.evaluate(() => {
    contribuicoesDosMods.registrar({ id: "mod/a" }, somA, {
      ponto: "pessoa.cartao",
      modo: "adicionar",
      alvo: "2",
      conteudo: [{ forma: "midia", chave: "chegada", fonte: "som/chegada.wav", tocando: true }],
    });
    desenhar(testTable.snapshot);
  });
  await esperarOSom("chegada", "tocando", "o som que a contribuição dá a Lia não tocou");
  const antesDeSair = await contagem();
  await pagina.evaluate(() => {
    const semLia = structuredClone(testTable.snapshot);
    for (const sala of semLia.voice_rooms) sala.people = sala.people.filter((pessoa) => pessoa.id !== 2);
    semLia.presentes = semLia.presentes.filter((pessoa) => pessoa.id !== 2);
    desenhar(semLia);
  });
  await esperarOSom(
    "chegada",
    "pausada",
    "Lia saiu do servidor, o nó dela saiu da lista, e o MOD não ouviu «pausada»: o som segue tocando para ninguém",
  );
  assert.equal(
    (await contagem()).paradas,
    antesDeSair.paradas + 1,
    "Lia saiu do servidor e a fonte do som dela não foi parada",
  );
  await pagina.evaluate(() => {
    contribuicoesDosMods.revogarDoMod("mod/a");
    desenhar(testTable.snapshot);
  });

  assert.deepEqual(recusasDaCsp, [], `a CSP recusou mídia: ${recusasDaCsp.join(" | ")}`);
  assert.deepEqual(erros, [], `a página lançou erro durante o som que sai da tela: ${erros.join(" | ")}`);
  await pagina.close();
}

const PROVAS = [
  aMidiaDeCadaPontoEContadaEDita,
  quemPintaCadaLugar,
  quemDesenhaOQueDizAEscolhaDestaMaquina,
  oModoDeDesenvolvedorContornaSemTomarNada,
  oSomDeModTocaPorWebAudioSobACspDoProduto,
  oSomQueSaiDaTelaPara,
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
