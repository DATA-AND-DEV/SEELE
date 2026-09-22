// A tela de qualidade de voz, na interface real. F01 e F02.
//
// O que ela prova, e é o que nenhum teste de Rust alcança:
//
// | O quê | Por que aqui |
// |---|---|
// | o marcador de redução de ruído reflete o que vale | o estado vem do Rust, e uma cópia em JavaScript discordaria depois de uma recarga |
// | a régua de sensibilidade usa os extremos do Rust | dois pares de extremos são dois pares esperando para discordar |
// | «padrão» aparece por extenso quando ninguém escolheu | um número que ninguém escolheu, mostrado como escolha, faz a pessoa achar que já mexeu |
// | o corte fica **na** barra do medidor | um medidor sem a marca é uma barra que se mexe; o que se ajusta é onde ela corta |
// | fechar a tela para o teste | um microfone aberto por uma tela que ninguém olha é um microfone esquecido |
//
// # O que ela não homologa
//
// Nada de áudio: não há microfone, não há supressão rodando, não há nível de
// verdade. A ponte é simulada e os números são os do teste. O que a avaliação
// acústica pede — gravações de fala baixa, teclado, ventilador, e CPU e latência
// medidas — continua pendente, e esta bancada não a substitui.
//
// Rodar:  node apps/seele-app/bancada/qualidade-da-voz.cjs

const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(
  root,
  "apps/seele-app/bancada/telas.cjs",
));
const chromium = require("./playwright.cjs").chromium();

/** Os controles como o Rust os manda, com o padrão de F01. */
function controles(extra = {}) {
  return {
    supressao: 1,
    abertura_dbfs: -60,
    abertura_escolhida: false,
    abertura_minima_dbfs: -72,
    abertura_maxima_dbfs: -24,
    ...extra,
  };
}

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
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
        previas_consentidas: [],
        limite_da_mensagem: 4096,
        controles_da_voz: controles(),
        estado_do_teste_de_microfone: {
          nivel_dbfs: -30,
          aberto: true,
          aberturas: 3,
          quadros_retidos: 6,
          // **Acima da régua de propósito.** O limiar acompanha o ruído medido, e
          // este número é o que decide — ver `corteMedidoDbfs`.
          corte_dbfs: -44,
          monitorando: false,
          falha: null,
        },
      }),
    );
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate((r) => {
      window.testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-server").hidden = false;
    }, retrato());

    // ---- F02: o marcador reflete o que vale, e não o que a marcação diz ----
    const filtro = await page.evaluate(async () => {
      window.testTable.controles_da_voz = {
        supressao: 0,
        abertura_dbfs: -42,
        abertura_escolhida: false,
        abertura_minima_dbfs: -72,
        abertura_maxima_dbfs: -24,
      };
      await desenharQualidadeDaVoz();
      return {
        marcado: $("server-reducao-de-ruido").checked,
        valor: $("server-sensibilidade-valor").textContent,
      };
    });
    assert.equal(
      filtro.marcado,
      false,
      "o marcador ficou ligado com a supressão desligada: ele está desenhando a " +
        "marcação em vez do que vale",
    );
    // **O padrão acompanhou.** É a decisão inteira de F01: −60 dBFS é do caminho
    // com filtro, e sem ele o limiar antigo volta.
    assert.match(
      filtro.valor,
      /-42\.0 dBFS · padrão/,
      `sem filtro o padrão deveria voltar a −42 dBFS: ${filtro.valor}`,
    );
    console.log("F02 OK: o marcador e o padrão da sensibilidade seguem o filtro");

    // ---- F01: a régua vem do Rust, e «padrão» é dito por extenso ----
    const regua = await page.evaluate(async () => {
      window.testTable.controles_da_voz = {
        supressao: 1,
        abertura_dbfs: -60,
        abertura_escolhida: false,
        abertura_minima_dbfs: -72,
        abertura_maxima_dbfs: -24,
      };
      await desenharQualidadeDaVoz();
      const controle = $("server-sensibilidade");
      return {
        min: controle.min,
        max: controle.max,
        valor: controle.value,
        frase: $("server-sensibilidade-valor").textContent,
        botaoDoPadrao: $("server-sensibilidade-padrao").hidden,
      };
    });
    assert.equal(regua.min, "-72", "o extremo sensível não veio do Rust");
    assert.equal(regua.max, "-24", "o extremo surdo não veio do Rust");
    assert.equal(regua.valor, "-60", "a régua não está no alvo de F01");
    assert.match(
      regua.frase,
      /padrão/,
      `o número aparece como escolha quando ninguém escolheu: ${regua.frase}`,
    );
    assert.equal(
      regua.botaoDoPadrao,
      true,
      "o botão de voltar ao padrão aparece quando já se está no padrão",
    );

    const escolhida = await page.evaluate(async () => {
      window.testTable.controles_da_voz = {
        supressao: 1,
        abertura_dbfs: -50,
        abertura_escolhida: true,
        abertura_minima_dbfs: -72,
        abertura_maxima_dbfs: -24,
      };
      await desenharQualidadeDaVoz();
      return {
        frase: $("server-sensibilidade-valor").textContent,
        botaoDoPadrao: $("server-sensibilidade-padrao").hidden,
      };
    });
    assert.doesNotMatch(
      escolhida.frase,
      /padrão/,
      `um valor escolhido está sendo chamado de padrão: ${escolhida.frase}`,
    );
    assert.equal(
      escolhida.botaoDoPadrao,
      false,
      "sem o botão de voltar ao padrão, a escolha fica presa: o padrão depende do " +
        "filtro, e um número congelado deixa de acompanhá-lo",
    );
    console.log("F01 OK: a régua vem do Rust e distingue escolha de padrão");

    // ---- O medidor, e a marca do corte dentro dele ----
    const medidor = await page.evaluate(async () => {
      window.testCalls.length = 0;
      await $("server-testar-microfone").click();
      await lerOTesteDeMicrofone();
      return {
        aberto: $("server-nivel-da-voz").hidden === false,
        largura: $("server-nivel-preenchido").style.width,
        corte: $("server-nivel-corte").style.left,
        estado: $("server-nivel-preenchido").dataset.aberto,
        frase: $("server-nivel-frase").textContent,
        pediu: window.testCalls.some((c) => c.cmd === "abrir_teste_de_microfone"),
        rotulo: $("server-testar-microfone").textContent.trim(),
        ouvir: $("server-ouvir-microfone").hidden,
      };
    });
    assert.equal(medidor.pediu, true, "o botão não abriu o teste");
    assert.equal(medidor.aberto, true, "o medidor não apareceu");
    assert.equal(medidor.rotulo, "PARAR O TESTE", "o botão não virou o de parar");
    assert.equal(medidor.ouvir, false, "OUVIR-ME não apareceu com o teste aberto");
    // −30 dBFS na faixa de −72 a −24: (−30 − −72) / 48 = 87,5%.
    assert.equal(medidor.largura, "87.5%", `a barra não bate com o nível: ${medidor.largura}`);
    // **E o corte é o medido, não o da régua.** A régua está em −50 dBFS, que
    // daria 45,8%; o corte medido é −44, que dá (−44 − −72) / 48 ≈ 58,3%. Se a
    // marca cair em 45,8% ela está desenhando o alvo enquanto o portão decide por
    // outro número — e quem abriu o teste para entender por que não abre vê a barra
    // passar de uma marca que não corta nada.
    assert.match(
      medidor.corte,
      /^58\.3/,
      `a marca do corte não seguiu o corte medido: ${medidor.corte}`,
    );
    assert.match(
      medidor.frase,
      /corte em -44\.0 dBFS pelo ruído da sala/,
      `a frase não conta que o corte subiu com o ruído: ${medidor.frase}`,
    );
    assert.equal(
      medidor.estado,
      "sim",
      "a barra não diz que está transmitindo, e a cor é a informação que some primeiro",
    );
    assert.match(
      medidor.frase,
      /TRANSMITINDO/,
      `a frase não diz o estado por extenso: ${medidor.frase}`,
    );
    assert.match(
      medidor.frase,
      /3 aberturas/,
      `a contagem de aberturas não aparece, e é ela que responde «o ventilador ` +
        `está abrindo isto?» sem ninguém ouvir: ${medidor.frase}`,
    );
    console.log("F01 OK: o medidor mostra o nível, o corte e quantas vezes abriu");

    // ---- Fechar a tela para o teste ----
    const fechou = await page.evaluate(async () => {
      window.testCalls.length = 0;
      fecharServerMesmo();
      // O `pararOTesteDeMicrofone` é assíncrono; duas voltas do laço bastam.
      await new Promise((ok) => setTimeout(ok, 0));
      await new Promise((ok) => setTimeout(ok, 0));
      return {
        parou: window.testCalls.some((c) => c.cmd === "fechar_teste_de_microfone"),
        rotulo: $("server-testar-microfone").textContent.trim(),
        medidor: $("server-nivel-da-voz").hidden,
      };
    });
    assert.equal(
      fechou.parou,
      true,
      "fechar a tela deixou o microfone aberto: um microfone aberto por uma tela " +
        "que ninguém está olhando é um microfone esquecido",
    );
    assert.equal(fechou.rotulo, "COMEÇAR O TESTE");
    assert.equal(fechou.medidor, true, "o medidor ficou na tela sem teste por trás");
    console.log("F01 OK: fechar a tela fecha o microfone");
  } finally {
    await browser.close();
    server.close();
  }
})().catch((erro) => {
  console.error(erro);
  process.exitCode = 1;
});
