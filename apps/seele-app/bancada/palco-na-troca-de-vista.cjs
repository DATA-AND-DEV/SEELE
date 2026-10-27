// **A tela assistida volta com a sessão** (achado do CUA na 0.17, três
// sessões: A no 18501 com a tela do helper À VISTA; a pessoa olha outra sessão
// e volta a A pela trilha; a imagem some, a legenda continua dizendo que «tela»
// está compartilhando e o seletor marca NÃO VER sem ninguém ter clicado nele).
//
// Bancada SINTÉTICA: o HTML e os scripts reais da casca do desktop
// (`verSessao`, `listen` do base.js, o palco, o seletor), a ponte do Tauri como
// duble que só anota e responde, e os eventos no envelope carimbado da 0.17
// (`{ sessao, geracao, vista, evento }`), na forma em que a FFI os solta: ela
// já filtra por leitura vigente — `ScreenOpened` a cada leitura nova,
// `ScreenClosed` só da vigente, quadro e som de leitura velha descartados (os
// testes `a_tela_ligada_pelo_servidor` do seele-ffi). O `VideoDecoder` é um
// duble que devolve um `VideoFrame` de verdade por quadro: o que se mede é a
// casca, e não o codec.
//
// # O defeito
//
// `ocultarOPalco` fechava o decodificador e zerava `telaEmCurso` ao sair da
// vista; o `listen` do base.js desviava para a placa os eventos de tela da
// sessão de fora (perdidos para o palco); e na volta `restaurarOPalco`
// devolvia só a escolha, esperando um `ScreenOpened` que ninguém reemitia — a
// sessão continua assinando a tela, e o servidor não reabre para quem já
// assiste. Todo quadro da volta caía no `tela !== telaEmCurso`, e o seletor lia
// `telaEmCurso === null` como NÃO VER.
//
// # Os casos (visibilidade COMPUTADA do canvas e do palco)
//
// 1. A volta com a mesma leitura religa: canvas e palco visíveis, um
//    quadro-chave pedido com o carimbo de A, o seletor em VER.
// 2. A mesma tela com uma LEITURA NOVA aberta enquanto A estava fora (medidas
//    novas): a volta abre a nova.
// 3. A transmissão encerrada enquanto fora: a volta diz que ninguém
//    compartilha, sem legenda ativa e sem canvas; nada é pedido.
// 4. NÃO VER explícito: a volta não abre nada e não manda nada — nem quando o
//    servidor reabre a tela enquanto A está fora.
// 5. Geração nova (reconexão enquanto fora): a abertura da geração velha não é
//    rearmada; a da geração nova, sim.
// 6. O retrato diz que há tela assistida e a janela não tem abertura para ela
//    (o registro esvaziado, como numa janela recarregada): a volta larga e pede
//    de novo a tela COM O ID DO RETRATO (pedir só de novo é ignorado pelo
//    servidor), e a abertura nova chega ao palco.
// 7. O estado do CUA (palco vazio, retrato com tela, ninguém escolheu): NÃO
//    VER larga a tela do retrato — antes não mandava nada —, e a volta depois
//    dele não reabre.
//
// Rodar:  node apps/seele-app/bancada/palco-na-troca-de-vista.cjs

const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const sessao = (id, nome) => ({
  carimbo: { sessao: id, geracao: 1 }, endereco: `${nome.toLowerCase()}.exemplo:8383`, nome,
  na_vista: id === 1, foco_de_voz: id === 1, na_sala_de_voz: id === 1 ? "Sala principal" : null,
  onde: "Outro", enlace: "online", encerrada: null, nao_lidas: 0, mudou: false,
});

const assistida = (tela) => ({ tela, de: 2, e_minha: false, espectadores: 2, parada: null, pedido: null });
const transmissao = (tela) => ({ tela, de: 2, e_minha: false, espectadores: 2, parada: null, assistida: true });

// A: na sala, assistindo a tela 5 da Lia (ou nenhuma, se ela acabou).
const retratoDeA = (tela = 5) => retrato({
  server: "A",
  tela: tela === null ? null : assistida(tela),
  transmissoes: tela === null ? [] : [transmissao(tela)],
});
const retratoDeC = () => retrato({
  server: "C",
  voice_rooms: [{ ...retrato().voice_rooms[0], occupied_by_us: false }],
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
      // A geração de cada sessão, como o Rust a carimba (sobe na reconexão).
      window.geracoes = { 1: 1, 3: 1 };
      // O retrato que cada sessão devolve agora (o caso o troca). O nome não
      // pode ser `retratos`: a casca tem um global com ele.
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
            // O servidor: quem já assiste e pede de novo é ignorado
            // (voice_room.rs, `assistir`); quem largou e pede de novo ganha uma
            // leitura nova, e a FFI solta o `ScreenOpened` dela. Só no caso 6.
            if (cmd === "assistir" && window.servidorDeVerdade) {
              const s = args?.carimbo?.sessao;
              if (!args.quero) window.largou = args.tela;
              else if (window.largou === args.tela) {
                window.largou = null;
                setTimeout(() => emitirDaSessao(s, { ScreenOpened: { screen: args.tela, width: 1280, height: 720 } }), 10);
              }
              return null;
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
      // O evento como o Rust o manda na 0.17: o envelope carimbado.
      window.emitirDaSessao = (sessao, evento) => {
        for (const fn of ouvintes.get("seele://event") ?? []) {
          fn({ event: "seele://event", id: 0, payload: { sessao, geracao: geracoes[sessao], vista: true, evento } });
        }
      };
      // Um quadro em Annex-B com um SPS (perfil High, nível 3.1), em base64.
      window.quadroChave = btoa(String.fromCharCode(0, 0, 0, 1, 0x67, 0x64, 0x00, 0x1f, 0xac, 0, 0, 0, 1, 0x65, 0x88));
      window.quadroDelta = btoa(String.fromCharCode(0, 0, 0, 1, 0x41, 0x9a));
      // O duble do decodificador: um `VideoFrame` de verdade por quadro.
      window.decodificados = 0;
      window.configurados = [];
      window.VideoDecoder = class {
        static async isConfigSupported(config) { return { supported: true, config }; }
        constructor({ output }) { this.output = output; this.state = "unconfigured"; }
        configure(config) { this.state = "configured"; configurados.push([config.codedWidth, config.codedHeight]); }
        decode() {
          decodificados += 1;
          const tela = new OffscreenCanvas(64, 36);
          tela.getContext("2d").fillRect(0, 0, 64, 36);
          this.output(new VideoFrame(tela, { timestamp: decodificados }));
        }
        close() { this.state = "closed"; }
      };
      window.EncodedVideoChunk = class { constructor(c) { Object.assign(this, c); } };
    }, {
      ...respostas({ microfones: [], saidas: [], meu_servidor: null }),
      snapshot: retratoDeA(),
      sessoes: [sessao(1, "A"), sessao(3, "C")],
      __a: retratoDeA(),
      __c: retratoDeC(),
    });
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.waitForTimeout(300);

    /** O que a pessoa vê, pelo estilo COMPUTADO e pela caixa, e o seletor. */
    const oQueSeVe = () => page.evaluate(() => {
      const visivel = (no) => {
        if (!no) return false;
        if (!no.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })) return false;
        const caixa = no.getBoundingClientRect();
        return caixa.width > 0 && caixa.height > 0;
      };
      const botoes = [...document.querySelectorAll("#palco-escolha button")];
      const marcado = botoes.find((b) => b.getAttribute("aria-pressed") === "true");
      return {
        canvas: visivel($("palco-imagem")),
        palco: visivel($("palco")),
        seletor: marcado ? marcado.textContent.trim() : null,
        telaEmCurso,
      };
    });
    const legenda = () => page.evaluate(() => ({
      quem: $("palco-quem").textContent.trim(),
      transmitindo: $("palco").dataset.transmitindo,
      falha: $("palco-falha").checkVisibility() ? $("palco-falha").textContent.trim() : null,
    }));
    /** Leva a vista a uma sessão e espera o desenho; abre a grade em A. */
    const ir = async (s) => {
      await page.evaluate(async (alvo) => {
        await verSessao(alvo);
        await atualizar();
      }, s);
      await page.waitForTimeout(100);
      await page.evaluate(async () => {
        if (vistaDaJanela.sessao === 1) {
          if ($("vista-chamada").hidden) await abrirChamada();
          else await atualizarChamada();
        }
      });
      await page.waitForTimeout(100);
    };
    const comandosDesde = (i) => page.evaluate((de) =>
      recebidos.slice(de).filter((r) => ["pedir_quadro_chave", "assistir"].includes(r.cmd))
        .map((r) => [r.cmd, r.args?.tela, r.args?.carimbo?.sessao ?? null, r.args?.quero ?? null]),
    i);
    const marco = () => page.evaluate(() => recebidos.length);
    /** Um quadro-chave de A chega (a resposta do helper ao pedido). */
    const chaveDeA = async (tela = 5) => {
      await page.evaluate((t) => emitirDaSessao(1, { ScreenFrame: { screen: t, key: true, data: quadroChave } }), tela);
      await page.waitForTimeout(100);
      await page.evaluate(async () => { await atualizar(); await atualizarChamada(); });
      await page.waitForTimeout(50);
    };
    /** A tela abre em A, na vista, e o primeiro quadro-chave chega. */
    const abrirEmA = async (tela = 5) => {
      await page.evaluate((t) => emitirDaSessao(1, { ScreenOpened: { screen: t, width: 1280, height: 720 } }), tela);
      await page.waitForTimeout(50);
      await chaveDeA(tela);
    };
    const caso = async (nome, corpo) => {
      try {
        await corpo();
        console.log(`ok — ${nome}`);
      } catch (falha) {
        falhas.push(nome);
        console.log(`FALHOU — ${nome}\n  ${falha.message.split("\n").join("\n  ")}`);
      }
    };
    const VENDO = { canvas: true, palco: true, seletor: "Lia", telaEmCurso: 5 };

    // A vista começa em A, na grade, com a tela da Lia à vista.
    await page.evaluate(async () => {
      for (const t of document.querySelectorAll("section.tela")) t.hidden = true;
      $("tela-sessao").hidden = false;
      await recarregarSessoes();
    });
    await ir(1);
    await abrirEmA();
    assert.deepEqual(await oQueSeVe(), VENDO, `a bancada não começou com a tela de A à vista: ${JSON.stringify(await oQueSeVe())}`);

    await caso("1. a volta com a mesma leitura religa o palco, e o seletor diz VER", async () => {
      await ir(3);
      const emC = await oQueSeVe();
      assert.equal(emC.canvas, false, `a imagem de A ficou à vista em C: ${JSON.stringify(emC)}`);
      // Enquanto A está fora da vista, os quadros dela continuam (desviados).
      await page.evaluate(() => emitirDaSessao(1, { ScreenFrame: { screen: 5, key: false, data: quadroDelta } }));
      const i = await marco();
      await ir(1);
      assert.deepEqual(await comandosDesde(i), [["pedir_quadro_chave", 5, 1, null]],
        `a volta não pediu UM quadro-chave da tela 5 com o carimbo de A: ${JSON.stringify(await comandosDesde(i))}`);
      await chaveDeA();
      const volta = await oQueSeVe();
      assert.deepEqual(volta, VENDO, `na volta a A, a imagem não voltou ou o seletor não está em VER: ${JSON.stringify(volta)}`);
    });

    await caso("2. a mesma tela com uma leitura nova aberta enquanto fora: a volta mostra a nova", async () => {
      await ir(3);
      // A FFI solta o `ScreenOpened` da leitura nova (a troca de par reabriu a
      // 5, agora em 1920×1080) enquanto A está fora da vista.
      await page.evaluate(() => emitirDaSessao(1, { ScreenOpened: { screen: 5, width: 1920, height: 1080 } }));
      const i = await marco();
      await page.evaluate(() => { configurados.length = 0; });
      await ir(1);
      assert.deepEqual(await comandosDesde(i), [["pedir_quadro_chave", 5, 1, null]],
        `a volta não pediu o quadro-chave da leitura nova: ${JSON.stringify(await comandosDesde(i))}`);
      await chaveDeA();
      assert.deepEqual(await page.evaluate(() => configurados), [[1920, 1080]],
        "o decodificador da volta não foi armado com as medidas da leitura nova");
      const volta = await oQueSeVe();
      assert.deepEqual(volta, VENDO, `a leitura nova não apareceu na volta: ${JSON.stringify(volta)}`);
    });

    await caso("3. a transmissão encerrada enquanto fora: a volta diz que ninguém compartilha, sem canvas", async () => {
      await ir(3);
      await page.evaluate(() => {
        emitirDaSessao(1, { ScreenClosed: { screen: 5 } });
        retratosDaPonte[1] = structuredClone(tabela.__a);
        retratosDaPonte[1].tela = null;
        retratosDaPonte[1].transmissoes = [];
      });
      const i = await marco();
      await ir(1);
      assert.deepEqual(await comandosDesde(i), [], `a volta pediu algo de uma tela encerrada: ${JSON.stringify(await comandosDesde(i))}`);
      await chaveDeA();
      const volta = await oQueSeVe();
      const dita = await legenda();
      assert.equal(volta.canvas, false, `um canvas apareceu para uma tela encerrada: ${JSON.stringify(volta)}`);
      assert.equal(volta.telaEmCurso, null, `a tela encerrada voltou ao palco: ${JSON.stringify(volta)}`);
      assert.equal(dita.transmitindo, "nao", `a legenda continua ativa sem transmissão: ${JSON.stringify(dita)}`);
      assert.equal(dita.quem, "NINGUÉM ESTÁ COMPARTILHANDO A TELA", `o palco não diz que ninguém compartilha: ${JSON.stringify(dita)}`);
      // A Lia volta a compartilhar, na vista: o caminho normal.
      await page.evaluate(() => { retratosDaPonte[1] = structuredClone(tabela.__a); });
      await abrirEmA();
      assert.deepEqual(await oQueSeVe(), VENDO, "a bancada não conseguiu devolver a tela a A");
    });

    await caso("4. com NÃO VER explícito, a volta não abre nada e não manda nada", async () => {
      await page.evaluate(async () => {
        [...document.querySelectorAll("#palco-escolha button")].find((b) => b.textContent.trim() === "NÃO VER").click();
        await new Promise((r) => setTimeout(r, 50));
        await atualizar();
        await atualizarChamada();
      });
      assert.equal((await oQueSeVe()).seletor, "NÃO VER", "o NÃO VER não ficou marcado em A");
      // O retrato do Rust, depois do NÃO VER: nenhuma assistida.
      await page.evaluate(() => { retratosDaPonte[1].tela = null; retratosDaPonte[1].transmissoes[0].assistida = false; });
      await ir(3);
      // O servidor reabre a 5 enquanto A está fora.
      await page.evaluate(() => emitirDaSessao(1, { ScreenOpened: { screen: 5, width: 1280, height: 720 } }));
      const i = await marco();
      await ir(1);
      assert.deepEqual(await comandosDesde(i), [], `a volta com NÃO VER mandou alguma coisa: ${JSON.stringify(await comandosDesde(i))}`);
      await chaveDeA();
      const volta = await oQueSeVe();
      assert.deepEqual(volta, { canvas: false, palco: true, seletor: "NÃO VER", telaEmCurso: null },
        `a volta religou contra o NÃO VER: ${JSON.stringify(volta)}`);
      // VER de novo, e a tela abre como o Rust a abriria.
      await page.evaluate(async () => {
        retratosDaPonte[1] = structuredClone(tabela.__a);
        [...document.querySelectorAll("#palco-escolha button")].find((b) => b.textContent.trim() !== "NÃO VER").click();
        await new Promise((r) => setTimeout(r, 50));
      });
      await abrirEmA();
      assert.equal((await oQueSeVe()).canvas, true, "a bancada não conseguiu devolver a tela a A");
    });

    await caso("5. geração nova: a abertura da velha não é rearmada, e a da nova sim", async () => {
      await ir(3);
      // A reconecta enquanto fora (geração 2); a ligada da 1 morreu no Rust, e
      // nenhuma leitura da 2 abriu ainda.
      await page.evaluate(() => {
        geracoes[1] = 2;
        retratosDaPonte[1] = structuredClone(tabela.__a);
        retratosDaPonte[1].tela = null;
      });
      let i = await marco();
      await ir(1);
      assert.deepEqual(await comandosDesde(i), [], `a geração 2 pediu algo pela abertura da 1: ${JSON.stringify(await comandosDesde(i))}`);
      const semAbertura = await oQueSeVe();
      assert.equal(semAbertura.telaEmCurso, null, `a geração 2 herdou o palco da 1: ${JSON.stringify(semAbertura)}`);
      assert.equal(semAbertura.canvas, false, `a geração 2 mostra imagem sem abertura: ${JSON.stringify(semAbertura)}`);
      // Fora de novo; a leitura da geração 2 abre enquanto A está fora.
      await ir(3);
      await page.evaluate(() => {
        retratosDaPonte[1] = structuredClone(tabela.__a);
        emitirDaSessao(1, { ScreenOpened: { screen: 5, width: 1280, height: 720 } });
      });
      i = await marco();
      await ir(1);
      assert.deepEqual(await comandosDesde(i), [["pedir_quadro_chave", 5, 1, null]],
        `a abertura da geração 2 não foi recuperada na volta: ${JSON.stringify(await comandosDesde(i))}`);
      const pedido = await page.evaluate(() => recebidos.filter((r) => r.cmd === "pedir_quadro_chave").at(-1).args.carimbo);
      assert.deepEqual(pedido, { sessao: 1, geracao: 2 }, `o pedido saiu com outra geração: ${JSON.stringify(pedido)}`);
      await chaveDeA();
      assert.deepEqual(await oQueSeVe(), VENDO, `a geração 2 não apareceu na volta: ${JSON.stringify(await oQueSeVe())}`);
    });

    await caso("6. há tela no retrato e nenhuma abertura: larga e pede de novo com o id do retrato, e a abertura chega", async () => {
      await ir(3);
      // Como numa janela recarregada: nenhuma abertura registrada para A.
      await page.evaluate(() => {
        if (typeof aberturaPorSessao !== "undefined") aberturaPorSessao.clear();
        window.servidorDeVerdade = true;
      });
      const i = await marco();
      await ir(1);
      await page.waitForTimeout(100);
      // E o `ScreenOpened` da leitura nova pede o quadro-chave dela.
      assert.deepEqual(await comandosDesde(i), [["assistir", 5, 1, false], ["assistir", 5, 1, true], ["pedir_quadro_chave", 5, 1, null]],
        `sem abertura, a volta não largou e pediu de novo a tela 5 do retrato: ${JSON.stringify(await comandosDesde(i))}`);
      // O servidor deu uma leitura nova; o `ScreenOpened` dela abriu o palco.
      await chaveDeA();
      const depois = await oQueSeVe();
      assert.deepEqual(depois, VENDO, `a abertura nova não chegou ao palco: ${JSON.stringify(depois)}`);
      assert.equal((await legenda()).falha, null, "a frase ficou à vista com a imagem de volta");
      await page.evaluate(() => { window.servidorDeVerdade = false; });
    });

    await caso("7. com o palco vazio e o retrato com tela (o estado do CUA), NÃO VER larga a tela do retrato, e a volta não reabre", async () => {
      // O estado medido no CUA às 07:55: legenda ativa (o retrato com a 5),
      // nenhuma imagem, ninguém escolheu. Posto à mão, porque é o que o
      // reparo acima impede de acontecer pela troca de vista.
      await page.evaluate(async () => {
        fecharImagemDaTela();
        telaQuerida = undefined;
        await atualizar();
        await atualizarChamada();
      });
      const cua = await oQueSeVe();
      assert.deepEqual([cua.canvas, cua.seletor, (await legenda()).transmitindo], [false, "NÃO VER", "sim"],
        `a bancada não reproduziu o estado do CUA: ${JSON.stringify(cua)}`);
      let i = await marco();
      await page.evaluate(async () => {
        [...document.querySelectorAll("#palco-escolha button")].find((b) => b.textContent.trim() === "NÃO VER").click();
        await new Promise((r) => setTimeout(r, 50));
      });
      assert.deepEqual(await comandosDesde(i), [["assistir", 5, 1, false]],
        `o NÃO VER com o palco vazio não largou a tela do retrato: ${JSON.stringify(await comandosDesde(i))}`);
      assert.equal(await page.evaluate(() => telaQuerida), null, "o NÃO VER não virou escolha");
      // O Rust, depois do NÃO VER: nenhuma assistida. Fora e de volta: nada reabre.
      await page.evaluate(() => { retratosDaPonte[1].tela = null; retratosDaPonte[1].transmissoes[0].assistida = false; });
      await ir(3);
      i = await marco();
      await ir(1);
      await page.waitForTimeout(100);
      assert.deepEqual(await comandosDesde(i), [], `a volta depois do NÃO VER mandou algo: ${JSON.stringify(await comandosDesde(i))}`);
      const volta = await oQueSeVe();
      assert.deepEqual(volta, { canvas: false, palco: true, seletor: "NÃO VER", telaEmCurso: null },
        `a volta reabriu contra o NÃO VER: ${JSON.stringify(volta)}`);
    });

    assert.deepEqual(erros, [], `erros na página: ${erros.join(" | ")}`);
  } finally {
    await browser.close();
    server.close();
  }
  if (falhas.length) {
    console.error(`palco na troca de vista: ${falhas.length} caso(s) falharam: ${falhas.join("; ")}`);
    process.exit(1);
  }
  console.log("palco na troca de vista: a tela assistida volta com a sessão (a mesma leitura e a nova; canvas e palco visíveis, seletor em VER, um quadro-chave pedido com o carimbo dela); a encerrada diz que ninguém compartilha; NÃO VER e a geração velha não abrem nada; sem abertura, larga e pede de novo com o id do retrato; NÃO VER com o palco vazio larga a tela do retrato.");
})().catch((e) => { console.error(e); process.exit(1); });
