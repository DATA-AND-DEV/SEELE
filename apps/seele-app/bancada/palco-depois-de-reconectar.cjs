// **O palco depois de reconectar o servidor inteiro** (frente «a tela
// desaparece / a voz para depois de reconectar», comunicacao-telas-escopo.md;
// o lado da UI, combinado com o hosting-lifecycle). Bancada SINTÉTICA: o HTML e
// os scripts reais da casca do desktop, a ponte do Tauri como duble, e os
// eventos de tela (`ScreenOpened`, `ScreenFrame`) entregues pelo duble do
// `listen`. Sem mídia, sem captura, sem codec de verdade.
//
// # A sequência do relato, na parte que é da janela
//
// 1. Sessão 1, na sala 7: a transmissão 5 abre, e a pessoa a escolhe (VER).
// 2. O servidor inteiro sai (o caminho do SAIR/RECONECTAR: `esquecerOEstadoDaSessao`).
// 3. Sessão 2 (geração nova), na MESMA sala 7, com a MESMA transmissão 5 no ar.
//
// # O que ela mede
//
// - Ao sair, o palco esquece a sessão: sem decodificador, sem tela em curso, a
//   imagem zerada e a escolha de volta a «ninguém escolheu».
// - Na sessão nova, o VER da transmissão 5 PEDE a assinatura (`assistir 5`):
//   antes, a escolha herdada fazia o clique cair no «já pedida», nenhum pedido
//   saía, e a imagem não voltava por clique nenhum.
// - O `ScreenOpened` da sessão nova abre a imagem e pede o quadro-chave.
// - OCULTAR não é ENCERRAR (revisão do Codex): A → B → A preserva o NÃO VER de
//   A e a escolha de B; a imagem sai da tela a cada troca; uma reconexão
//   (geração nova) não herda a escolha guardada.
// - Uma promessa velha (a MESMA transmissão 5, a geração anterior) que volta
//   depois da abertura nova não arma decodificador, não destrava a armação nova
//   e não entrega bytes; o decodificador de uma abertura fechada não pinta nem
//   fala. Sintética: um `VideoDecoder` de mentira com a consulta segurada.
//
// Não é reprodução nativa: nada aqui passa por servidor, transporte ou codec
// de verdade.
//
// Rodar:  node apps/seele-app/bancada/palco-depois-de-reconectar.cjs

const assert = require("node:assert/strict");
const { servir, respostas } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const TRANSMISSAO = { tela: 5, de: 2, e_minha: false, espectadores: 1, parada: null };
const retratoNaSala = () => ({
  voice_rooms: [{ id: 7, name: "Sala", limit: 8, password_required: false, occupied_by_us: true, channel: 1,
    people: [{ id: 1, nickname: "eu" }, { id: 2, nickname: "lia" }], sync: null }],
  presentes: [{ id: 1, nickname: "eu" }, { id: 2, nickname: "lia" }],
  transmissoes: [TRANSMISSAO],
});

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    const erros = [];
    page.on("pageerror", (e) => erros.push(e.message));
    await page.addInitScript((tabela) => {
      window.tabela = tabela;
      window.chamadas = [];
      window.ouvintes = {};
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => {
          chamadas.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
          return structuredClone(tabela[cmd] ?? null);
        } },
        event: { listen: async (nome, f) => { (ouvintes[nome] ??= []).push(f); return () => {}; } },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
      // Um evento da sessão, como o Rust o entrega à janela.
      window.emitir = (carga) => { for (const f of ouvintes["seele://event"] ?? []) f({ payload: carga }); };
    }, respostas({}));
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.waitForTimeout(300);

    const ler = () => page.evaluate(() => ({
      emCurso: telaEmCurso,
      querida: telaQuerida === undefined ? "ninguém escolheu" : telaQuerida,
      sala: salaDaEscolha,
      temDecodificador: decodificador !== null,
      imagem: { escondida: $("palco-imagem").hidden, largura: $("palco-imagem").width },
    }));
    const assinaturas = () => page.evaluate(() => chamadas.filter((c) => c.cmd === "assistir").map((c) => c.args));

    // 1. Sessão 1: a transmissão abre, e a pessoa a escolhe.
    await page.evaluate((r) => {
      entrarNaGeracao(1);
      desenharTransmissoes(r);
      emitir({ ScreenOpened: { screen: 5, width: 1280, height: 720 } });
    }, retratoNaSala());
    await page.evaluate(async (r) => {
      // NÃO VER e VER: a escolha fica sendo a 5.
      await pararDeVer();
      desenharTransmissoes(r);
      await trocarDeTransmissao(5);
    }, retratoNaSala());
    let antes = await ler();
    assert.equal(antes.querida, 5, `a escolha da sessão 1 não é a 5: ${JSON.stringify(antes)}`);

    // 2. O servidor inteiro sai.
    await page.evaluate(() => esquecerOEstadoDaSessao());
    const depois = await ler();
    assert.deepEqual(depois, {
      emCurso: null, querida: "ninguém escolheu", sala: null, temDecodificador: false,
      imagem: { escondida: true, largura: 0 },
    }, `o palco não esqueceu a sessão que saiu: ${JSON.stringify(depois)}`);

    // 3. Sessão 2, mesma sala, mesma transmissão: o VER pede a assinatura.
    await page.evaluate((r) => {
      chamadas.length = 0;
      entrarNaGeracao(2);
      desenharTransmissoes(r);
    }, retratoNaSala());
    await page.evaluate(() => {
      const ver = [...document.querySelectorAll("#palco-escolha button")].find((b) => /lia/.test(b.textContent));
      ver.click();
    });
    await page.waitForTimeout(100);
    assert.deepEqual(await assinaturas(), [{ tela: 5, quero: true }],
      "na sessão nova, o VER da mesma transmissão não pediu a assinatura (a escolha da sessão velha ficou)");

    // O `ScreenOpened` da sessão nova abre a imagem e pede o quadro-chave.
    await page.evaluate(() => { chamadas.length = 0; emitir({ ScreenOpened: { screen: 5, width: 1280, height: 720 } }); });
    await page.waitForTimeout(100);
    assert.equal(await page.evaluate(() => telaEmCurso), 5, "o ScreenOpened da sessão nova não abriu a imagem");
    assert.deepEqual(await page.evaluate(() => chamadas.filter((c) => c.cmd === "pedir_quadro_chave").map((c) => c.args)),
      [{ tela: 5 }], "a sessão nova não pediu o quadro-chave");

    // 4. OCULTAR não é ENCERRAR (revisão do Codex): A → B → A preserva a escolha
    //    de cada uma; a imagem sai da tela a cada troca.
    const trocar = (sessao, geracao) => page.evaluate(({ sessao, geracao }) => {
      fecharOQueEDaSessao();
      esquecerOEstadoDaSessao();
      porNaVista({ sessao, geracao });
      restaurarOEstadoDaVista(sessao);
    }, { sessao, geracao });
    await page.evaluate(async () => {
      porNaVista({ sessao: 1, geracao: 1 });
      await pararDeVer(); // A escolhe NÃO VER
    });
    await trocar(2, 1);
    const emB = await ler();
    assert.equal(emB.emCurso, null, `a imagem de A ficou na tela de B: ${JSON.stringify(emB)}`);
    assert.equal(emB.querida, "ninguém escolheu", `B herdou a escolha de A: ${JSON.stringify(emB)}`);
    await page.evaluate(async () => { await trocarDeTransmissao(9); }); // B escolhe a 9
    await trocar(1, 1);
    assert.equal((await ler()).querida, null, "voltando a A, o NÃO VER de A se perdeu");
    await trocar(2, 1);
    assert.equal((await ler()).querida, 9, "voltando a B, a escolha da 9 se perdeu");

    // 5. Reconectar A (geração nova) NÃO herda a escolha guardada da geração anterior.
    await trocar(1, 2);
    assert.equal((await ler()).querida, "ninguém escolheu", "a reconexão de A herdou a escolha da geração anterior");

    // 6. A promessa velha (a MESMA transmissão 5, a geração anterior) não arma
    //    decodificador nem entrega bytes na sessão nova.
    await page.evaluate(() => {
      // Um `VideoDecoder` de mentira, com o `isConfigSupported` segurado pela bancada.
      window.instancias = [];
      window.consultas = [];
      window.VideoDecoder = class {
        constructor(cb) { this.cb = cb; this.entregues = []; instancias.push(this); }
        configure(c) { this.config = c; }
        decode(chunk) { this.entregues.push(chunk.type); }
        close() { this.fechado = true; }
        static isConfigSupported(config) { return new Promise((ok, no) => consultas.push({ ok, no, config })); }
      };
      window.EncodedVideoChunk = class { constructor(o) { Object.assign(this, o); } };
      // Um quadro-chave com SPS (High, nível 3.1), em base64.
      window.chave = btoa(String.fromCharCode(0, 0, 0, 1, 0x67, 0x64, 0x00, 0x1f, 0xac, 0, 0, 0, 1, 0x65, 0x88));
      esquecerOEstadoDaSessao();
      entrarNaGeracao(10);
      emitir({ ScreenOpened: { screen: 5, width: 1280, height: 720 } });
      emitir({ ScreenFrame: { screen: 5, key: true, data: chave } });
    });
    await page.waitForFunction(() => consultas.length === 1);
    // O servidor inteiro sai e volta, e a MESMA transmissão 5 abre na geração nova.
    await page.evaluate(() => {
      esquecerOEstadoDaSessao();
      entrarNaGeracao(11);
      emitir({ ScreenOpened: { screen: 5, width: 1280, height: 720 } });
      emitir({ ScreenFrame: { screen: 5, key: true, data: chave } });
    });
    await page.waitForFunction(() => consultas.length === 2);
    // A promessa VELHA volta primeiro, dizendo que suporta: nada pode nascer dela.
    await page.evaluate(() => consultas[0].ok({ supported: true }));
    await page.waitForTimeout(50);
    let estado = await page.evaluate(() => ({ instancias: instancias.length, decodificador: decodificador !== null, armando }));
    assert.deepEqual(estado, { instancias: 0, decodificador: false, armando: true },
      `a promessa da geração anterior armou o decodificador na sessão nova, ou destravou a armação nova: ${JSON.stringify(estado)}`);
    // A nova volta: um decodificador, e o quadro-chave entregue a ele.
    await page.evaluate(() => consultas[1].ok({ supported: true }));
    await page.waitForFunction(() => instancias.length === 1);
    estado = await page.evaluate(() => ({ entregues: instancias[0].entregues, decodificador: decodificador === instancias[0], armando }));
    assert.deepEqual(estado, { entregues: ["key"], decodificador: true, armando: false }, `a armação nova: ${JSON.stringify(estado)}`);

    // 6b. O decodificador de uma abertura que já fechou não pinta nem apaga nada.
    await page.evaluate(() => {
      window.velho = instancias[0];
      esquecerOEstadoDaSessao();
      entrarNaGeracao(12);
      emitir({ ScreenOpened: { screen: 5, width: 1280, height: 720 } });
      window.fechouOQuadro = false;
      velho.cb.output({ displayWidth: 640, displayHeight: 360, close() { window.fechouOQuadro = true; } });
      velho.cb.error(new Error("tarde"));
    });
    estado = await page.evaluate(() => ({
      fechouOQuadro, largura: $("palco-imagem").width, emCurso: telaEmCurso, falha: $("palco-falha")?.hidden ?? true,
    }));
    assert.deepEqual(estado, { fechouOQuadro: true, largura: 0, emCurso: 5, falha: true },
      `o decodificador velho pintou ou falou na abertura nova: ${JSON.stringify(estado)}`);

    assert.deepEqual(erros, [], `erros na página: ${erros.join(" | ")}`);
  } finally {
    await browser.close();
    server.close();
  }
  console.log("palco depois de reconectar: encerrar esquece e a sessão nova assina de novo; ocultar guarda a escolha (A→B→A); a reconexão não herda; a promessa e o decodificador velhos não tocam na abertura nova.");
})().catch((e) => { console.error(e); process.exit(1); });
