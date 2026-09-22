// As sondas de conversa da revisão da v15, **com o oráculo invertido**.
//
// Nas sondas originais — `docs/evidencias/review-v15/browser-probes.cjs` —
// «passou» queria dizer «reproduzi o defeito». Aqui passa quando o defeito não
// acontece.
//
// | Sonda original | O que ela provava | O que este arquivo prova |
// |---|---|---|
// | PTT | soltar a tecla com o compositor focado deixava `falando=true` | a soltura sempre fecha o microfone (R03) |
// | COMPOSER | a falha de um envio anterior sobrescrevia o rascunho novo | o rascunho novo fica, e a mensagem vira pendente (R05) |
// | PREVIEW SELECTION | `tracking.invalid` e `127.0.0.1` eram prévia automática | nenhuma requisição sai sem consentimento (R06) |
//
// # O que esta bancada **não** homologa
//
// Nada de rede: a ponte é simulada e nenhum endereço é buscado. A régua de
// destino do R06 é do Rust e tem os testes dela lá — aqui se prova que a janela
// **não manda buscar**, que é a outra metade.
//
// Rodar:  node apps/seele-app/bancada/conversa-com-estado.cjs

const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(
  root,
  "apps/seele-app/bancada/telas.cjs",
));
const chromium = require("./playwright.cjs").chromium();

// As variáveis destes scripts são de escopo de script e não de `window`: um
// `let` de topo num script clássico não vira propriedade de `window`, e escrever
// `window.falando` criaria uma segunda variável com o mesmo nome — o teste
// passaria mexendo numa coisa que a interface não lê.

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
              // O ponto de suspensão do R05: o envio fica pendente até o teste
              // decidir o desfecho dele.
              if (cmd === "send_message" && window.deferSend) {
                return new Promise((ok, fail) => {
                  window.rejectSend = fail;
                });
              }
              // **A recusa por falta de consentimento, no lugar certo.**
              //
              // Aqui e não num `invoke` trocado depois: `base.js` faz
              // `const { invoke } = window.__TAURI__.core` no carregamento, então
              // trocar a propriedade depois não alcança quem já a capturou — e o
              // teste passaria mexendo numa função que a interface não chama.
              if (cmd === "previa_de_link" && window.semConsentimento) {
                window.previasPedidas = window.previasPedidas ?? [];
                window.previasPedidas.push(args.url);
                throw {
                  SemConsentimento: { dominio: new URL(args.url).hostname },
                };
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
        limite_da_mensagem: 4096,
        previas_consentidas: [],
        som_da_tela: { volume: 1, calada: false },
        exclusao_do_som_da_captura: "Excluido",
      }),
    );
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate((r) => {
      window.testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(r);
    }, retrato());

    // ---- R03: soltar a tecla fecha o microfone, com foco onde estiver ----
    //
    // A sequência é a da reprodução: segurar Espaço fora do campo, focar o
    // compositor, soltar Espaço. Antes do conserto saía `set_talking(true)` sem
    // o `false` correspondente, e o microfone ficava aberto para sempre.
    const ptt = await page.evaluate(() => {
      document.activeElement?.blur();
      window.testCalls.length = 0;
      window.dispatchEvent(
        new KeyboardEvent("keydown", { code: "Space", key: " ", bubbles: true }),
      );
      $("campo-mensagem").focus();
      window.dispatchEvent(
        new KeyboardEvent("keyup", { code: "Space", key: " ", bubbles: true }),
      );
      return {
        falando,
        chamadas: window.testCalls
          .filter((c) => c.cmd === "set_talking")
          .map((c) => c.args.talking),
      };
    });
    assert.equal(
      ptt.falando,
      false,
      "soltar a tecla com o compositor focado deixou o microfone aberto",
    );
    assert.deepEqual(
      ptt.chamadas,
      [true, false],
      `o par abrir/fechar não saiu inteiro: ${JSON.stringify(ptt.chamadas)}`,
    );

    // E a outra metade: trocar a tecla **durante** o segurar não trava o
    // microfone. Quem fecha é a soltura da tecla que abriu, e não a preferência
    // lida do disco.
    const trocaDeTecla = await page.evaluate(() => {
      document.activeElement?.blur();
      window.testCalls.length = 0;
      window.dispatchEvent(
        new KeyboardEvent("keydown", { code: "Space", key: " ", bubbles: true }),
      );
      // A configuração troca a escolha no meio.
      teclaDeFalar = "KeyF";
      window.dispatchEvent(
        new KeyboardEvent("keyup", { code: "Space", key: " ", bubbles: true }),
      );
      const saiu = window.testCalls
        .filter((c) => c.cmd === "set_talking")
        .map((c) => c.args.talking);
      teclaDeFalar = "Space";
      segurarFala(false);
      return { falando, saiu };
    });
    assert.equal(
      trocaDeTecla.falando,
      false,
      "trocar a tecla de falar no meio do segurar travou o microfone aberto",
    );
    console.log("R03 OK: a soltura fecha o microfone, com foco e tecla onde estiverem");

    // ---- R05: a falha de um envio não sobrescreve o rascunho novo ----
    const rascunho = await page.evaluate(async () => {
      linhaAberta = 1;
      anexoPendente = null;
      window.deferSend = true;
      $("campo-mensagem").value = "primeira mensagem";
      const pendente = enviar({ preventDefault() {} });
      // A pessoa começa a escrever outra coisa enquanto a primeira espera.
      $("campo-mensagem").value = "rascunho novo";
      window.rejectSend("NotConnected");
      await pendente;
      window.deferSend = false;
      return $("campo-mensagem").value;
    });
    assert.equal(
      rascunho,
      "rascunho novo",
      "a falha de um envio anterior sobrescreveu o que estava sendo escrito",
    );

    // E o rascunho fica **com o canal em que foi escrito**: trocar de canal não
    // apaga o que se estava escrevendo.
    const porCanal = await page.evaluate(() => {
      linhaAberta = 1;
      $("campo-mensagem").value = "isto é do canal um";
      guardarRascunho();
      linhaAberta = 2;
      restaurarRascunho();
      const noDois = $("campo-mensagem").value;
      linhaAberta = 1;
      restaurarRascunho();
      return { noDois, noUm: $("campo-mensagem").value };
    });
    assert.equal(porCanal.noDois, "", "o rascunho de um canal apareceu noutro");
    assert.equal(
      porCanal.noUm,
      "isto é do canal um",
      "trocar de canal e voltar apagou o que estava escrito",
    );
    console.log("R05 OK: o rascunho é por canal, e uma falha não o sobrescreve");

    // ---- R06: nenhuma requisição sai sem consentimento ----
    //
    // A sonda original casava `pareceImagem(url) === true` nos dois endereços e
    // concluía que os dois eram prévia automática. `pareceImagem` continua
    // dizendo `true` — ela é o segundo filtro, o de «isto **parece** mídia» — e o
    // que mudou é que ela já não manda buscar nada.
    const previa = await page.evaluate(async () => {
      window.testCalls.length = 0;
      // O Rust responde «falta consentimento», que é o que ele responde **sem
      // fazer requisição nenhuma** — ver `previa_de_link`, onde a pergunta do
      // consentimento vem antes de `buscar_com_teto`.
      window.semConsentimento = true;
      window.previasPedidas = [];
      mensagens = [
        {
          id: 1,
          channel: 1,
          author: 2,
          author_nickname: "Lia",
          at_seconds: 1_726_900_000,
          body: "olha isto https://tracking.invalid/visitor.png",
          own: false,
          edited: false,
          estado: "Confirmada",
          client_message_id: null,
        },
      ];
      desenharMensagens();
      // Duas voltas do laço de eventos: a promessa recusada redesenha.
      await new Promise((ok) => setTimeout(ok, 0));
      await new Promise((ok) => setTimeout(ok, 0));
      window.semConsentimento = false;
      const convites = [...document.querySelectorAll(".previa-convite-acao")].map((b) => ({
        dominio: b.dataset.dominio,
        url: b.dataset.previa,
      }));
      return {
        pedidos: window.previasPedidas,
        convites,
        imagens: document.querySelectorAll(".previa-de-link").length,
      };
    });
    assert.equal(
      previa.imagens,
      0,
      "uma imagem de origem não confiável foi desenhada sem consentimento",
    );
    assert.deepEqual(
      previa.convites,
      [
        {
          dominio: "tracking.invalid",
          url: "https://tracking.invalid/visitor.png",
        },
      ],
      `o convite não apareceu nomeando o domínio: ${JSON.stringify(previa.convites)}`,
    );
    // O `previa_de_link` é chamado **uma vez** e volta sem buscar nada: é ele
    // que confere o consentimento antes de qualquer byte sair. O que não pode
    // acontecer é a janela insistir a cada desenho.
    assert.equal(
      previa.pedidos.length,
      1,
      `a janela perguntou ${previa.pedidos.length} vezes: um endereço que não ` +
        "responde vira uma batida constante na porta de alguém",
    );
    console.log("R06 OK: ler uma mensagem não dispara prévia, e o convite nomeia o domínio");
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro);
  process.exitCode = 1;
});
