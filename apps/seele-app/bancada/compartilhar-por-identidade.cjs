// As três sondas de compartilhamento da revisão da v15, **com o oráculo
// invertido**.
//
// Nas sondas originais — `docs/evidencias/review-v15/screen-share-probes.cjs` —
// «passou» queria dizer «reproduzi o defeito». Aqui passa quando o defeito não
// acontece. É a inversão que a própria revisão pede para as sondas virarem
// testes permanentes de regressão.
//
// | Sonda original | O que ela provava | O que este arquivo prova |
// |---|---|---|
// | MULTIPLE SHARES | outra pessoa transmitindo desabilitava `COMPARTILHAR` | o botão continua ligado; «cabe uma por vez» não existe mais (R18) |
// | CINEMA | parar de assistir deixava o cinema de pé, escondendo a navegação | parar sai do cinema e devolve a navegação (R19) |
// | STOP RACE | um pedido obsoleto assinava depois do clique em «não ver» | o pedido obsoleto é descartado (R16) |
//
// # O que esta bancada **não** homologa
//
// Nada de mídia nativa: sem captura, sem codec, sem transporte, sem tela cheia
// de verdade. É a interface real com a ponte simulada — o mesmo alcance das
// sondas que ela substitui, e a homologação entre duas máquinas continua
// pendente.
//
// Rodar:  node apps/seele-app/bancada/compartilhar-por-identidade.cjs

const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(
  root,
  "apps/seele-app/bancada/telas.cjs",
));
const chromium = require("./playwright.cjs").chromium();

// **As variáveis do palco são de escopo de script, e não de `window`.**
//
// `palco-imagem.js` as declara com `let` no topo de um script clássico, e um
// `let` de topo não vira propriedade de `window`. Escrever `window.telaQuerida`
// criaria uma segunda variável com o mesmo nome — o teste passaria mexendo numa
// coisa que a interface não lê. Por isso as atribuições abaixo são nuas.

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    page.on("pageerror", (erro) => {
      throw erro;
    });
    await page.addInitScript(
      (table) => {
        window.testTable = table;
        window.testCalls = [];
        window.__TAURI__ = {
          core: {
            invoke: async (cmd, args) => {
              window.testCalls.push({ cmd, args });
              // O ponto de suspensão da corrida do R16: o cancelamento da
              // transmissão anterior fica pendente até o teste o concluir.
              if (cmd === "assistir" && args.quero === false && window.deferStop) {
                return new Promise((ok) => {
                  window.finishStop = ok;
                });
              }
              return Object.hasOwn(table, cmd) ? table[cmd] : null;
            },
          },
          event: { listen: async () => () => {} },
          window: {
            getCurrentWindow: () => ({
              onCloseRequested() {},
              isMaximized: async () => false,
            }),
          },
        };
      },
      respostas({
        snapshot: null,
        microfones: [],
        saidas: [],
        pacotes_no_cache: [],
        aceites_de_mods: [],
        exclusao_do_som_da_captura: "Excluido",
        som_da_tela: { volume: 1, calada: false },
        limite_da_mensagem: 4096,
        previas_consentidas: [],
      }),
    );
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate((r) => {
      window.testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(r);
    }, retrato());

    // ---- R18: outra pessoa transmitindo não bloqueia mais o botão ----
    //
    // A sonda original casava `disabled === true` e o título «Cabe uma por
    // vez». Aqui o oráculo é o contrário, e a segunda asserção é a que impede o
    // conserto de virar «desabilita por outro motivo»: a frase não pode voltar.
    const varios = await page.evaluate(() => {
      fonteArmada = 1;
      desenharBotoesDeTela({
        ...window.testTable.snapshot,
        // A de outra pessoa está no ar, e a minha não existe.
        tela: { tela: 9, de: 2, e_minha: false, espectadores: 1, parada: null, pedido: null },
        minha_transmissao: null,
        transmissoes: [{ tela: 9, de: 2, e_minha: false, assistida: true, espectadores: 1 }],
      });
      const botao = $("compartilhar-comecar");
      return {
        disabled: botao.disabled,
        title: botao.title,
        rotulo: botao.textContent.trim(),
        erro: $("compartilhar-erro").hidden,
      };
    });
    assert.equal(
      varios.disabled,
      false,
      "a interface voltou a bloquear a segunda transmissão: o servidor guarda " +
        "várias por sala, e a admissão depende de capacidade de banda",
    );
    assert.equal(
      varios.rotulo,
      "COMPARTILHAR",
      "o botão deixou de oferecer começar enquanto outra pessoa transmite",
    );
    assert(
      !varios.title.includes("uma por vez"),
      `a frase de um limite que o servidor não tem voltou: ${varios.title}`,
    );
    assert.equal(
      varios.erro,
      true,
      "a caixa escreve uma recusa sem ninguém ter apertado nada",
    );
    console.log("R18 OK: duas pessoas podem transmitir, e a janela não afirma o contrário");

    // ---- R19: parar de assistir sai do cinema ----
    const cinema = await page.evaluate(async () => {
      window.testTable.snapshot.tela = { tela: 7, de: 2, e_minha: false, espectadores: 1, parada: null, pedido: null };
      window.testTable.snapshot.minha_transmissao = null;
      window.testTable.snapshot.transmissoes = [
        { tela: 7, de: 2, e_minha: false, assistida: true, espectadores: 1 },
      ];
      desenhar(window.testTable.snapshot);
      abrirChamada();
      telaEmCurso = 7;
      telaQuerida = 7;
      await trocarCinema(true);
      await pararDeVer();
      // Quem transmite continua transmitindo, e é esta a condição do defeito:
      // `botaoDeCinema` só saía quando não havia transmissão no palco.
      botaoDeCinema(true);
      return {
        noCinema: noCinema,
        telaEmCurso: telaEmCurso,
        escondidos: document.querySelectorAll('[data-fora-do-cinema="sim"]').length,
        corpo: document.body.dataset.cinema ?? null,
      };
    });
    assert.equal(cinema.telaEmCurso, null, "a imagem não parou");
    assert.equal(
      cinema.noCinema,
      false,
      "parar de assistir deixou o cinema de pé: a navegação fica escondida por " +
        "CSS com nada na frente dela",
    );
    assert.equal(cinema.corpo, null, "o corpo continua marcado como cinema");
    assert.equal(
      cinema.escondidos,
      0,
      `${cinema.escondidos} elementos de navegação continuam marcados para ocultação`,
    );
    console.log("R19 OK: parar de assistir devolve a navegação");

    // ---- R16: o pedido obsoleto é descartado ----
    const corrida = await page.evaluate(async () => {
      await trocarCinema(false);
      window.testCalls.length = 0;
      telaEmCurso = 7;
      telaQuerida = 7;
      window.deferStop = true;
      // Pede B. O cancelamento de A fica pendente dentro desta promessa.
      const trocando = trocarDeTransmissao(8);
      // E, no meio dele, a pessoa clica em «não ver».
      const parando = pararDeVer();
      window.deferStop = false;
      window.finishStop();
      await trocando;
      await parando;
      return {
        telaQuerida: telaQuerida ?? null,
        assinaturas: window.testCalls.filter(
          (c) => c.cmd === "assistir" && c.args.quero === true,
        ),
      };
    });
    assert.equal(
      corrida.telaQuerida,
      null,
      "a escolha de não assistir foi desfeita por uma operação em curso",
    );
    assert.deepEqual(
      corrida.assinaturas,
      [],
      "um pedido obsoleto reativou a assinatura depois do clique em «não ver»: " +
        `${JSON.stringify(corrida.assinaturas)}`,
    );
    console.log("R16 OK: nenhum pedido obsoleto reativa a assinatura");
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro);
  process.exitCode = 1;
});
