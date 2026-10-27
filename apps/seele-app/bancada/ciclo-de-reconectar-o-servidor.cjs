// **O ciclo do relato, pela TELA, gravando os `invoke`** (pedido do
// hosting-lifecycle, frente «a voz para depois de reconectar o servidor
// inteiro»). Bancada SINTÉTICA: o HTML e os scripts reais da casca do desktop
// (o `conectar`, o ENTRAR da sala, o palco, o SAIR DO SERVIDOR), a ponte do
// Tauri como duble que só anota e responde. Não é reprodução nativa: nada aqui
// passa por Rust, transporte, áudio ou codec.
//
// # O ciclo
//
// conectar (sessão 1) → ENTRAR na sala → a tela da Lia abre e é assistida (VER)
// → SAIR DO SERVIDOR → conectar de novo (sessão 2, o `SessaoId` novo que o Rust
// dá) → ENTRAR na sala.
//
// # O que ela mostra (e afirma)
//
// 1. Depois de voltar, o `enter_voice_room` sai com o carimbo NOVO (sessão 2), e
//    nenhum comando da volta sai com o carimbo da sessão 1.
// 2. Da tela assistida da sessão velha não sobra nada vivo NA JANELA: sem
//    decodificador, sem tela em curso, sem escolha guardada, a imagem zerada, e
//    nenhum `assistir`/`pedir_quadro_chave` sai depois do SAIR.
// 3. O que a janela pede na volta, em ordem, vai impresso (o «registro» do
//    ciclo), para o hosting-lifecycle comparar com o que o Rust recebe.
//
// Rodar:  node apps/seele-app/bancada/ciclo-de-reconectar-o-servidor.cjs [--json saída]

const fs = require("node:fs");
const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const fora = (extra = {}) => retrato({
  voice_rooms: [{
    id: 1, name: "Sala principal", limit: 8, password_required: false, occupied_by_us: false, channel: 1,
    people: [{ id: 2, nickname: "Lia", speaking: false, muted: false, total_isolation: false, signal: 90, sync_band: "Nominal", is_self: false }],
    sync: null,
  }],
  transmissoes: [{ tela: 5, de: 2, e_minha: false, espectadores: 1, parada: null }],
  ...extra,
});

(async () => {
  const saida = process.argv.includes("--json") ? process.argv[process.argv.indexOf("--json") + 1] : null;
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    const erros = [];
    page.on("pageerror", (e) => erros.push(e.message));
    await page.addInitScript((base) => {
      window.registro = [];
      window.ouvintes = new Map();
      window.sessaoDoConnect = 1;
      const tabela = { ...base };
      window.tabela = tabela;
      window.__TAURI__ = {
        core: {
          invoke: async (cmd, args) => {
            const copia = args === undefined ? undefined : JSON.parse(JSON.stringify(args));
            registro.push({ cmd, args: copia });
            if (cmd === "connect") {
              const s = sessaoDoConnect;
              tabela.snapshot = structuredClone(base.__fora);
              return {
                carimbo: { sessao: s, geracao: 1 }, vista: 10 + s, geracao: 10 + s, snapshot: structuredClone(base.__fora),
                veredito: "Known", origem_da_impressao: "Lista", a_lista_guardou: false,
              };
            }
            if (cmd === "enter_voice_room") {
              tabela.snapshot.voice_rooms[0].occupied_by_us = true;
              return null;
            }
            if (cmd === "disconnect") {
              tabela.snapshot = null;
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
      // Um evento da sessão como o Rust o manda na 0.17: o envelope carimbado.
      window.emitirDaSessao = (sessao, evento) => {
        for (const fn of ouvintes.get("seele://event") ?? []) {
          fn({ event: "seele://event", id: 0, payload: { sessao, geracao: 1, vista: true, evento } });
        }
      };
    }, {
      ...respostas({ microfones: [], saidas: [], meu_servidor: null, sessoes: null, onde_estou: "Outro" }),
      __fora: fora(),
    });
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.waitForTimeout(300);

    const marca = (nome) => page.evaluate((n) => registro.push({ marca: n }), nome);
    const entrarNaSala = async () => {
      await page.waitForSelector('#lista-voice_rooms button[data-voice_room="1"]', { state: "attached", timeout: 5000 });
      await page.evaluate(() => document.querySelector('#lista-voice_rooms button[data-voice_room="1"]').click());
      await page.waitForFunction(() => registro.some((r) => r.cmd === "enter_voice_room" && !r.lido), null, { timeout: 5000 });
      await page.evaluate(() => { for (const r of registro) if (r.cmd === "enter_voice_room") r.lido = true; });
    };
    const conectarPelaTela = async (sessao) => {
      await page.evaluate(async (s) => {
        sessaoDoConnect = s;
        await conectar("casa.exemplo:8383", "Alex");
      }, sessao);
      await page.waitForFunction(() => !$("tela-sessao").hidden || !$("tela-auth").hidden, null, { timeout: 5000 });
      // A chave conhecida passa pela autenticação sem pergunta; se a tela dela
      // estiver à frente, segue pelo botão dela, como a pessoa faria.
      await page.evaluate(() => { if (!$("tela-auth").hidden) document.querySelector("#tela-auth button.botao-auth")?.click(); });
      await page.waitForFunction(() => !$("tela-sessao").hidden, null, { timeout: 5000 });
    };

    // --- sessão 1
    await marca("conectar (sessão 1)");
    await conectarPelaTela(1);
    await marca("ENTRAR na sala (sessão 1)");
    await entrarNaSala();
    await marca("a tela da Lia abre, e VER");
    await page.evaluate(async () => {
      emitirDaSessao(1, { ScreenOpened: { screen: 5, width: 1280, height: 720 } });
      await pararDeVer();
      desenharTransmissoes(tabela.snapshot);
      await trocarDeTransmissao(5);
      emitirDaSessao(1, { ScreenOpened: { screen: 5, width: 1280, height: 720 } });
    });
    await page.waitForTimeout(100);
    assert.equal(await page.evaluate(() => telaEmCurso), 5, "a tela da sessão 1 não abriu no palco");

    // --- sair do servidor inteiro
    await marca("SAIR DO SERVIDOR");
    await page.evaluate(() => sairDoServidorParaAEntrada());
    await page.waitForFunction(() => !$("tela-boot").hidden, null, { timeout: 5000 });
    const depoisDeSair = await page.evaluate(() => ({
      telaEmCurso, decodificador: decodificador !== null,
      escolha: telaQuerida === undefined ? "ninguém escolheu" : telaQuerida,
      guardadas: palcoPorSessao.size, imagem: $("palco-imagem").width, vista: vistaDaJanela.sessao,
    }));
    const indiceDaSaida = await page.evaluate(() => registro.length);

    // --- voltar
    await marca("conectar (sessão 2)");
    await conectarPelaTela(2);
    await marca("ENTRAR na sala (sessão 2)");
    await entrarNaSala();
    await page.waitForTimeout(200);

    const { volta, todos } = await page.evaluate((i) => ({ volta: registro.slice(i), todos: registro }), indiceDaSaida);
    const entradasNaSala = todos.filter((r) => r.cmd === "enter_voice_room").map((r) => r.args?.carimbo ?? null);
    const comCarimboVelho = volta.filter((r) => r.args?.carimbo?.sessao === 1).map((r) => r.cmd);
    const midiaVelha = volta.filter((r) => ["assistir", "pedir_quadro_chave"].includes(r.cmd));

    const relatorio = {
      entradasNaSala, depoisDeSair, comCarimboVelho, midiaDepoisDeSair: midiaVelha,
      volta: volta.map((r) => (r.marca ? `— ${r.marca}` : `${r.cmd}${r.args?.carimbo ? ` @${r.args.carimbo.sessao}/${r.args.carimbo.geracao}` : ""}`)),
    };
    console.log(JSON.stringify(relatorio, null, 2));
    if (saida) fs.writeFileSync(saida, JSON.stringify(relatorio, null, 2));

    // 1.
    assert.deepEqual(entradasNaSala, [{ sessao: 1, geracao: 1 }, { sessao: 2, geracao: 1 }],
      `o enter_voice_room da volta não saiu com o carimbo novo: ${JSON.stringify(entradasNaSala)}`);
    assert.deepEqual(comCarimboVelho, [], `comandos da volta com o carimbo da sessão 1: ${comCarimboVelho}`);
    // 2.
    assert.deepEqual(depoisDeSair, {
      telaEmCurso: null, decodificador: false, escolha: "ninguém escolheu", guardadas: 0, imagem: 0, vista: null,
    }, `a tela assistida da sessão velha deixou algo vivo na janela: ${JSON.stringify(depoisDeSair)}`);
    assert.deepEqual(midiaVelha.filter((r) => r.args?.carimbo?.sessao !== 2), [],
      `assinatura ou quadro-chave da sessão velha depois do SAIR: ${JSON.stringify(midiaVelha)}`);
    assert.deepEqual(erros.filter((e) => !/VideoDecoder/.test(e)), [], `erros na página: ${erros.join(" | ")}`);
  } finally {
    await browser.close();
    server.close();
  }
  console.log("ciclo de reconectar o servidor: a volta entra na sala com o carimbo novo, nenhum comando sai com o velho, e a tela da sessão velha não deixa nada vivo na janela.");
})().catch((e) => { console.error(e); process.exit(1); });
