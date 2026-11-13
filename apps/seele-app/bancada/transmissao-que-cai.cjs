// **A transmissão de tela que cai acaba na tela também, e diz por quê.**
//
// # O defeito
//
// Medido num Windows com o codificador de software forçado: a bomba de tela
// morre no meio da transmissão, sai um `warn` no `seele.log`, e a janela segue
// como estava — o botão em PARAR DE COMPARTILHAR sobre uma transmissão que
// ninguém vê, e nenhuma frase. O produto sabia e não contava.
//
// O núcleo agora encerra a transmissão (o servidor ouve o `StopScreenShare`, e
// o retrato perde a `minha_transmissao`) e solta um `ScreenShareFell` com o que
// ela mostrava e o nome do motivo. Esta bancada mede a metade da janela.
//
// Bancada SINTÉTICA: o HTML e os scripts reais da casca do desktop, a ponte do
// Tauri como duble que só anota e responde, e os eventos no envelope carimbado
// (`{ sessao, geracao, vista, evento }`), na forma em que o Rust os solta. O
// que se lê é o que a pessoa vê: a visibilidade COMPUTADA da frase, o texto
// dela, e o nome acessível do botão.
//
// # Os casos
//
// 1. No ar: o botão diz PARAR DE COMPARTILHAR, e não há frase.
// 2. A queda, **antes** de o retrato perder a transmissão (é a ordem em que o
//    Rust os manda): a frase do motivo aparece, e fica. Quando o retrato a
//    perde, o botão volta a COMPARTILHAR e a frase continua lá.
// 3. Cada motivo que o núcleo enumera tem a frase dele, e um nome que esta
//    janela não conhece diz que caiu sem inventar a causa.
// 4. Nada recomeça sozinho: nenhum `compartilhar_tela` sai da janela.
// 5. A pessoa compartilha de novo (outra transmissão dela no retrato): a frase
//    some.
// 6. Sair da sala também a apaga.
// 7. A câmera que cai não escreve na linha da tela.
// 8. A queda numa sessão FORA da vista não aparece na que está à vista, e está
//    lá quando a pessoa volta à sessão dela.
// 9. A página do navegador: o `ScreenClosed` com `erro` da tela diz a frase
//    que veio nele; o da câmera, não.
//
// Rodar:  node apps/seele-app/bancada/transmissao-que-cai.cjs

const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const sessao = (id, nome) => ({
  carimbo: { sessao: id, geracao: 1 }, endereco: `${nome.toLowerCase()}.exemplo:8383`, nome,
  na_vista: id === 1, foco_de_voz: id === 1, na_sala_de_voz: "Sala principal",
  onde: "Outro", enlace: "online", encerrada: null, nao_lidas: 0, mudou: false,
});

/** A transmissão desta pessoa, como o retrato a traz. */
const minha = (tela) => ({ tela, de: 1, e_minha: true, espectadores: 1, parada: null, pedido: null });
/** O retrato de uma sessão: na sala, compartilhando a `tela` (ou nada). */
const retratoCom = (server, tela, extra = {}) => retrato({
  server,
  minha_transmissao: tela === null ? null : minha(tela),
  tela: tela === null ? null : minha(tela),
  transmissoes: tela === null ? [] : [{ tela, de: 1, e_minha: true, espectadores: 1, parada: null, assistida: false }],
  ...extra,
});

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  const falhas = [];
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    const erros = [];
    page.on("pageerror", (e) => erros.push(e.message));
    if (process.argv.includes("--depurar")) page.on("console", (m) => console.log(`[página] ${m.type()}: ${m.text()}`));
    await page.addInitScript((base) => {
      window.recebidos = [];
      window.ouvintes = new Map();
      const tabela = { ...base };
      window.tabela = tabela;
      window.geracoes = { 1: 1, 3: 1 };
      // O retrato que cada sessão devolve agora (o caso o troca).
      window.retratosDaPonte = { 1: base.__a, 3: base.__c };
      window.__TAURI__ = {
        core: {
          invoke: async (cmd, args) => {
            recebidos.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
            if (cmd === "ver_sessao") {
              tabela.sessoes = tabela.sessoes.map((s) => ({
                ...s, na_vista: s.carimbo.sessao === args.sessao,
                carimbo: { sessao: s.carimbo.sessao, geracao: geracoes[s.carimbo.sessao] },
              }));
              tabela.snapshot = structuredClone(retratosDaPonte[args.sessao]);
              window.vistaVez = (window.vistaVez ?? 100) + 1;
              return {
                carimbo: { sessao: args.sessao, geracao: geracoes[args.sessao] },
                geracao: vistaVez, snapshot: structuredClone(retratosDaPonte[args.sessao]), veredito: "Known",
              };
            }
            if (cmd === "snapshot") {
              const s = args?.carimbo?.sessao;
              return structuredClone(s ? retratosDaPonte[s] : tabela.snapshot);
            }
            return structuredClone(tabela[cmd] ?? null);
          },
        },
        event: {
          listen: async (nome, fn) => {
            if (!ouvintes.has(nome)) ouvintes.set(nome, []);
            ouvintes.get(nome).push(fn);
            return () => {};
          },
        },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
      // O evento como o Rust o manda: o envelope carimbado.
      window.emitirDaSessao = (sessao, evento) => {
        for (const fn of ouvintes.get("seele://event") ?? []) {
          fn({ event: "seele://event", id: 0, payload: { sessao, geracao: geracoes[sessao], vista: true, evento } });
        }
      };
    }, {
      ...respostas({ microfones: [], saidas: [], meu_servidor: null, permissao_de_tela: "Concedida" }),
      snapshot: retratoCom("A", 5),
      sessoes: [sessao(1, "A"), sessao(3, "C")],
      __a: retratoCom("A", 5),
      __c: retratoCom("C", 9),
    });
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.waitForTimeout(300);

    /** O que a pessoa vê: a frase (só se VISÍVEL de fato) e o botão. */
    const oQueSeVe = () => page.evaluate(() => {
      const visivel = (no) => {
        if (!no) return false;
        if (!no.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })) return false;
        const caixa = no.getBoundingClientRect();
        return caixa.width > 0 && caixa.height > 0;
      };
      const linha = $("tela-estado");
      const botao = $("operador-vista");
      return {
        frase: visivel(linha) ? linha.textContent : null,
        papel: linha.getAttribute("role"),
        botao: botao.getAttribute("aria-label"),
        apertado: botao.getAttribute("aria-pressed"),
        modo: botao.dataset.modo,
        botaoVisivel: visivel(botao),
      };
    });
    /** O retrato que o Rust devolve para a sessão passa a ser este, e a janela o relê. */
    const retratoPassaASer = async (s, novo) => {
      await page.evaluate(([alvo, r]) => { retratosDaPonte[alvo] = r; }, [s, novo]);
      await page.evaluate(async () => { await atualizar(); });
      await page.waitForTimeout(80);
    };
    const emitir = async (s, evento) => {
      await page.evaluate(([alvo, e]) => emitirDaSessao(alvo, e), [s, evento]);
      await page.waitForTimeout(120);
    };
    const ir = async (s) => {
      await page.evaluate(async (alvo) => { await verSessao(alvo); await atualizar(); }, s);
      await page.waitForTimeout(120);
    };
    const frases = () => page.evaluate(() => ({ ...QUEDAS, desconhecida: QUEDA_DESCONHECIDA }));
    const comecos = () => page.evaluate(() => recebidos.filter((r) => ["compartilhar_tela", "compartilhar_camera"].includes(r.cmd)).length);
    const caso = async (nome, corpo) => {
      try {
        await corpo();
        console.log(`ok — ${nome}`);
      } catch (falha) {
        falhas.push(nome);
        console.log(`FALHOU — ${nome}\n  ${falha.message.split("\n").join("\n  ")}`);
      }
    };

    // A vista começa em A, na sala, compartilhando a tela 5.
    await page.evaluate(async () => {
      for (const t of document.querySelectorAll("section.tela")) t.hidden = true;
      $("tela-sessao").hidden = false;
      await recarregarSessoes();
    });
    await ir(1);
    const QUEDAS = await frases();
    assert.deepEqual(Object.keys(QUEDAS).sort(), ["Captura", "Codificador", "Fluxo", "desconhecida"],
      `o dicionário de quedas não é o que esta bancada mede: ${Object.keys(QUEDAS)}`);

    await caso("1. no ar: o botão diz PARAR DE COMPARTILHAR, e não há frase", async () => {
      const visto = await oQueSeVe();
      assert.deepEqual([visto.botao, visto.apertado, visto.modo, visto.botaoVisivel, visto.frase],
        ["PARAR DE COMPARTILHAR", "true", "parar", true, null],
        `a bancada não começou com a tela no ar: ${JSON.stringify(visto)}`);
      assert.equal(visto.papel, "status", "a linha da queda não é uma região viva: quem não vê não ficaria sabendo");
    });

    await caso("2. a queda aparece antes de o retrato perder a transmissão, e fica depois", async () => {
      // A ordem do Rust: o `ScreenShareFell` sai logo depois do `StopScreenShare`,
      // e o `ScreenShareStopped` que tira a transmissão do retrato volta depois.
      await emitir(1, { ScreenShareFell: { screen: 5, source: "monitor", reason: "Codificador" } });
      const naHora = await oQueSeVe();
      assert.equal(naHora.frase, QUEDAS.Codificador,
        `a transmissão caiu e a janela não disse: ${JSON.stringify(naHora)}`);
      // O laço da chamada relê o retrato, que ainda traz a transmissão: a frase não pode sumir por isso.
      await page.evaluate(async () => { await atualizar(); });
      await page.waitForTimeout(80);
      assert.equal((await oQueSeVe()).frase, QUEDAS.Codificador,
        "a frase sumiu num redesenho com a transmissão ainda no retrato: ela piscaria e ninguém a leria");
      // O servidor devolveu o fim: o retrato não tem mais a transmissão.
      await retratoPassaASer(1, retratoCom("A", null));
      await emitir(1, "ScreenChanged");
      const depois = await oQueSeVe();
      assert.deepEqual([depois.botao, depois.apertado, depois.modo, depois.frase],
        ["COMPARTILHAR", "false", "compartilhar", QUEDAS.Codificador],
        `depois da queda o botão não voltou a COMPARTILHAR, ou a frase sumiu: ${JSON.stringify(depois)}`);
    });

    await caso("3. cada motivo tem a frase dele, e o que a janela não conhece não inventa a causa", async () => {
      for (const [motivo, frase] of [["Captura", QUEDAS.Captura], ["Fluxo", QUEDAS.Fluxo], ["Codificador", QUEDAS.Codificador], ["MotivoDoFuturo", QUEDAS.desconhecida]]) {
        await emitir(1, { ScreenShareFell: { screen: 5, source: "window", reason: motivo } });
        assert.equal((await oQueSeVe()).frase, frase, `a queda por \`${motivo}\` não disse a frase dela`);
      }
      const ditas = new Set([QUEDAS.Captura, QUEDAS.Fluxo, QUEDAS.Codificador, QUEDAS.desconhecida]);
      assert.equal(ditas.size, 4, "dois motivos dizem a mesma frase");
      for (const frase of ditas) {
        assert.match(frase, /CAIU/, `a frase não diz que a transmissão caiu: ${frase}`);
        assert.doesNotMatch(frase, /volta sozinha/i, `a frase promete uma volta que não existe: ${frase}`);
      }
    });

    await caso("4. nada recomeça sozinho", async () => {
      assert.equal(await comecos(), 0, "a janela pediu uma transmissão nova sem ninguém apertar");
    });

    await caso("5. compartilhar de novo apaga a frase", async () => {
      await retratoPassaASer(1, retratoCom("A", 6));
      await emitir(1, "ScreenChanged");
      const visto = await oQueSeVe();
      assert.deepEqual([visto.botao, visto.frase], ["PARAR DE COMPARTILHAR", null],
        `com outra transmissão no ar a frase da queda anterior continua escrita: ${JSON.stringify(visto)}`);
    });

    await caso("6. sair da sala apaga a frase", async () => {
      await emitir(1, { ScreenShareFell: { screen: 6, source: "monitor", reason: "Captura" } });
      await retratoPassaASer(1, retratoCom("A", null));
      assert.equal((await oQueSeVe()).frase, QUEDAS.Captura, "a queda da segunda transmissão não foi dita");
      const fora = retratoCom("A", null);
      fora.voice_rooms[0].occupied_by_us = false;
      await retratoPassaASer(1, fora);
      assert.equal((await oQueSeVe()).frase, null, "fora da sala a frase da queda continua escrita");
      // De volta à sala, sem transmissão: nada a dizer.
      await retratoPassaASer(1, retratoCom("A", null));
      assert.equal((await oQueSeVe()).frase, null, "a frase voltou sozinha ao entrar de novo na sala");
    });

    await caso("7. a câmera que cai não escreve na linha da tela", async () => {
      await emitir(1, { ScreenShareFell: { screen: 7, source: "camera", reason: "Fluxo" } });
      assert.equal((await oQueSeVe()).frase, null, "a queda da câmera foi dita como queda da tela");
    });

    await caso("8. a queda numa sessão fora da vista espera a volta dela", async () => {
      // A vista está em A; a transmissão de C (a tela 9) cai lá.
      await emitir(3, { ScreenShareFell: { screen: 9, source: "monitor", reason: "Fluxo" } });
      assert.equal((await oQueSeVe()).frase, null, "a queda de C foi escrita na vista de A");
      await page.evaluate(() => { retratosDaPonte[3].minha_transmissao = null; retratosDaPonte[3].tela = null; retratosDaPonte[3].transmissoes = []; });
      await ir(3);
      const emC = await oQueSeVe();
      assert.deepEqual([emC.botao, emC.frase], ["COMPARTILHAR", QUEDAS.Fluxo],
        `na volta a C a queda dela não está dita: ${JSON.stringify(emC)}`);
      await ir(1);
      assert.equal((await oQueSeVe()).frase, null, "a queda de C veio junto para a vista de A");
    });

    await caso("9. a página do navegador: o `ScreenClosed` com erro da tela diz a frase dele; o da câmera, não", async () => {
      const daPagina = "A imagem compartilhada parou. Compartilhe a tela novamente.";
      await emitir(1, { ScreenClosed: { screen: 11, erro: "A câmera parou.", camera: true } });
      assert.equal((await oQueSeVe()).frase, null, "o fim com erro da câmera foi dito na linha da tela");
      await emitir(1, { ScreenClosed: { screen: 11 } });
      assert.equal((await oQueSeVe()).frase, null, "um fim de transmissão sem erro foi dito como queda");
      await emitir(1, { ScreenClosed: { screen: 11, erro: daPagina, camera: false } });
      assert.equal((await oQueSeVe()).frase, daPagina, "o fim com erro da tela, na página, ficou mudo");
    });

    await caso("10. na vista da chamada a frase continua à vista, onde quer que os controles estejam", async () => {
      // A grade da chamada pode tirar o operador de cena e pôr os comandos numa
      // bandeja própria: a frase tem de estar à vista num dos dois lugares.
      await page.evaluate(async () => { if ($("vista-chamada").hidden) await abrirChamada(); });
      await page.waitForTimeout(150);
      await emitir(1, { ScreenShareFell: { screen: 12, source: "monitor", reason: "Captura" } });
      const vistas = await page.evaluate(() => ["tela-estado", "chamada-tela-estado"].map((id) => {
        const no = $(id);
        if (!no || !no.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })) return null;
        const caixa = no.getBoundingClientRect();
        return caixa.width > 0 && caixa.height > 0 ? no.textContent : null;
      }));
      assert.ok(vistas.includes(QUEDAS.Captura),
        `na vista da chamada a queda não está à vista em lugar nenhum: ${JSON.stringify(vistas)}`);
    });

    assert.equal(await comecos(), 0, "a janela pediu uma transmissão nova sem ninguém apertar");
    assert.deepEqual(erros, [], `erros na página: ${erros.join(" | ")}`);
  } finally {
    await browser.close();
    server.close();
  }
  if (falhas.length) {
    console.error(`transmissão que cai: ${falhas.length} caso(s) falharam: ${falhas.join("; ")}`);
    process.exit(1);
  }
  console.log("transmissão que cai: a queda é dita ao lado do COMPARTILHAR, com a frase do motivo (codificador, captura, conexão, e a de quem não conhece o motivo); o botão volta a COMPARTILHAR com o retrato e a frase fica; nada recomeça sozinho; compartilhar de novo e sair da sala apagam a frase; a câmera não escreve na linha da tela; a queda de uma sessão fora da vista espera a volta dela; e a página do navegador diz o erro que veio no fim da tela.");
})().catch((e) => { console.error(e); process.exit(1); });
