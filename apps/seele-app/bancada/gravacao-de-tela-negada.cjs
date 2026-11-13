// **A gravação de tela que o macOS negou, na caixa de compartilhar.**
//
// Relato do dono em 13/11/2026, com a 1.0.0: «mesmo que eu permita o
// compartilhamento de tela nos Ajustes, ele não funciona, não habilita nunca,
// fica num loop infinito». Medido na mesma data com esta UI e o contrato da
// 1.0.0: depois de cada pedido negado a caixa voltava a «ESTE APP AINDA NÃO
// PEDIU PARA GRAVAR A TELA», com o botão de pedir, e a frase de «negada» (a
// única com o caminho dos Ajustes) nunca aparecia.
//
// A UI é a real; a ponte é um dublê do aplicativo **com o contrato do núcleo de
// agora** (`seele-core/src/video.rs`, `MemoriaDoPedidoDeTela`): a consulta diz
// `NaoPerguntada` antes de um pedido e `Negada` depois de um pedido negado, e o
// macOS deste dublê nunca concede (é o caso da permissão presa à versão
// anterior). O que ela prende:
//
//  1. Abrir a caixa não pede nada ao sistema: mostra PEDIR AO SISTEMA e espera.
//  2. O pedido negado tira o botão de pedir e põe a verdade no lugar: a
//     permissão pode ter ficado presa a uma versão anterior, o caminho nos
//     Ajustes com o nome que o macOS escreve, e os três botões.
//  3. **Sem laço**: redesenhar, fechar e abrir a caixa, e atualizar a chamada
//     não pedem de novo, e a caixa não volta a oferecer o pedido.
//  4. ABRIR OS AJUSTES chama o comando uma vez e diz que abriu; se o sistema
//     não abre, a caixa diz, com o caminho à mão.
//  5. REDEFINIR A PERMISSÃO só roda no aperto, uma vez; a falha é dita; depois
//     dela a caixa só oferece reabrir, e não pede de novo.
//  6. REABRIR O SEELE reabre; com um servidor no ar, pergunta antes, pela caixa
//     de confirmação, e só reabre no sim.
//  7. Fora do macOS a negação não ganha estes botões, e concedida não diz nada.
//
//   PLAYWRIGHT=<playwright-core> node apps/seele-app/bancada/gravacao-de-tela-negada.cjs

const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(root, "apps/seele-app/bancada/telas.cjs"));
const chromium = require("./playwright.cjs").chromium();

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const erros = [];
    page.on("pageerror", (erro) => erros.push(String(erro)));
    await page.addInitScript(
      (table) => {
        window.testTable = table;
        window.testCalls = [];
        // O macOS de mentira, visto por um processo: o que o sistema responde
        // agora, o que o núcleo guardou do pedido, e o que cada saída fez.
        window.mac = { concedida: false, negou: false, falhar: {}, servidor: null };
        const comandos = {
          permissao_de_tela: () => (window.mac.concedida ? "Concedida" : window.mac.negou ? "Negada" : "NaoPerguntada"),
          pedir_permissao_de_tela: () => {
            window.mac.negou = !window.mac.concedida;
            return window.mac.concedida ? "Concedida" : "Negada";
          },
          abrir_ajustes_da_gravacao_de_tela: () => null,
          redefinir_permissao_de_tela: () => null,
          reabrir_o_app: () => null,
          meu_servidor: () => window.mac.servidor,
        };
        window.__TAURI__ = {
          core: {
            invoke: async (cmd, args) => {
              window.testCalls.push({ cmd, args });
              if (Object.hasOwn(window.mac.falhar, cmd)) throw window.mac.falhar[cmd];
              if (Object.hasOwn(comandos, cmd)) return comandos[cmd]();
              return Object.hasOwn(table, cmd) ? table[cmd] : null;
            },
          },
          event: { listen: async () => () => {} },
          window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
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
        fontes_de_tela: [],
      }),
    );
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.evaluate((r) => {
      window.testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(r);
    }, retrato());
    await page.waitForFunction(() => $("barra-janela").dataset.plataforma === "macos");

    const vezes = (cmd) => page.evaluate((c) => window.testCalls.filter((x) => x.cmd === c).length, cmd);
    const caixa = () =>
      page.evaluate(() => {
        const visto = (id) => {
          const no = $(id);
          return !no.hidden && no.getClientRects().length > 0;
        };
        return {
          aberta: !$("compartilhar").hidden,
          bloco: visto("compartilhar-permissao"),
          diz: $("compartilhar-permissao-diz").textContent,
          nota: $("compartilhar-permissao-nota").textContent,
          pedir: visto("compartilhar-pedir"),
          passos: visto("compartilhar-permissao-passos")
            ? [...$("compartilhar-permissao-passos").children].map((li) => li.textContent)
            : [],
          ajustes: visto("compartilhar-abrir-ajustes"),
          redefinir: visto("compartilhar-redefinir"),
          reabrir: visto("compartilhar-reabrir"),
          oQueFazem: visto("compartilhar-permissao-o-que-fazem") ? $("compartilhar-permissao-o-que-fazem").textContent : null,
          feito: visto("compartilhar-permissao-feito") ? $("compartilhar-permissao-feito").textContent : null,
          erro: visto("compartilhar-erro") ? $("compartilhar-erro").textContent : null,
        };
      });
    const dito = (v) => JSON.stringify(v);

    // 1. Abrir não pede nada ao sistema.
    await page.evaluate(() => abrirCompartilhar());
    let v = await caixa();
    assert.equal(await vezes("pedir_permissao_de_tela"), 0, "abrir a caixa pediu a permissão sem ninguém apertar");
    assert.ok(v.bloco && v.pedir, `sem permissão, a caixa não ofereceu o pedido: ${dito(v)}`);
    assert.match(v.diz, /AINDA NÃO LIBEROU A GRAVAÇÃO DE TELA/);
    assert.ok(!v.ajustes && !v.redefinir && !v.reabrir && v.passos.length === 0, `as saídas apareceram antes de qualquer pedido: ${dito(v)}`);

    // 2. O pedido negado: a verdade, o caminho e as três saídas.
    await page.locator("#compartilhar-pedir").click();
    await page.waitForFunction(() => !$("compartilhar-permissao-saidas").hidden);
    v = await caixa();
    assert.equal(await vezes("pedir_permissao_de_tela"), 1);
    assert.ok(!v.pedir, `depois do não, a caixa continuou oferecendo o mesmo pedido: ${dito(v)}`);
    assert.match(v.diz, /O MACOS NÃO LIBEROU A GRAVAÇÃO DE TELA PARA O SEELE/);
    assert.match(v.nota, /versão anterior/, "a nota não diz que a permissão pode ter ficado presa à versão anterior");
    assert.match(v.nota, /atualização/);
    assert.equal(v.passos.length, 4, `os passos: ${dito(v.passos)}`);
    assert.match(v.passos[0], /Ajustes do Sistema › Privacidade e Segurança › Gravação do Áudio do Sistema e da Tela/);
    assert.match(v.passos[0], /«Gravação de Tela»/, "falta o nome da página nas versões anteriores do macOS");
    assert.match(v.passos[2], /«−».*«\+»/, "falta o remover e adicionar de novo");
    assert.match(v.passos[3], /Reabra o SEELE\. A permissão só vale depois de reabrir\./);
    assert.ok(v.ajustes && v.redefinir && v.reabrir, `faltou uma das três saídas: ${dito(v)}`);
    assert.match(v.passos[2], /Desligar e ligar a chave pode não bastar/);
    assert.match(v.oQueFazem, /REDEFINIR A PERMISSÃO apaga só a decisão do macOS sobre o SEELE gravar a tela/);
    assert.match(v.oQueFazem, /REABRIR O SEELE fecha e abre o app: você sai da sala/);
    for (const frase of [v.diz, v.nota, ...v.passos, v.oQueFazem]) {
      assert.ok(frase.length <= 180, `uma frase da caixa passou de 180 caracteres (${frase.length}): ${frase}`);
    }
    assert.equal(await vezes("abrir_ajustes_da_gravacao_de_tela"), 0);
    assert.equal(await vezes("redefinir_permissao_de_tela"), 0, "a permissão foi redefinida sem ninguém apertar");
    assert.equal(await vezes("reabrir_o_app"), 0);

    if (process.env.SEELE_PREVIEW_DIR) {
      await page.screenshot({ path: path.join(process.env.SEELE_PREVIEW_DIR, "gravacao-de-tela-negada.png") });
    }

    // 3. Sem laço: nada do que redesenha pede de novo, nem devolve o pedido.
    await page.evaluate(() => desenharCompartilhar());
    await page.evaluate(() => desenharCompartilhar({ relistar: true }));
    await page.evaluate(() => atualizarChamada());
    await page.evaluate(() => fecharCompartilhar());
    await page.evaluate(() => abrirCompartilhar());
    v = await caixa();
    assert.equal(await vezes("pedir_permissao_de_tela"), 1, "um pedido negado disparou outro");
    assert.ok(!v.pedir && v.ajustes && v.redefinir && v.reabrir && v.passos.length === 4, `a caixa saiu do estado negado sozinha: ${dito(v)}`);

    // 4. ABRIR OS AJUSTES: uma vez, e dito. Quando não abre, dito também.
    await page.locator("#compartilhar-abrir-ajustes").click();
    await page.waitForFunction(() => !$("compartilhar-permissao-feito").hidden);
    v = await caixa();
    assert.equal(await vezes("abrir_ajustes_da_gravacao_de_tela"), 1);
    assert.match(v.feito, /Os Ajustes do Sistema foram abertos/);
    assert.equal(v.erro, null);
    await page.evaluate(() => { window.mac.falhar.abrir_ajustes_da_gravacao_de_tela = "AjustesNaoAbriram"; });
    await page.locator("#compartilhar-abrir-ajustes").click();
    await page.waitForFunction(() => !$("compartilhar-erro").hidden);
    v = await caixa();
    assert.match(v.erro, /OS AJUSTES DO SISTEMA NÃO ABRIRAM/);
    assert.match(v.erro, /Abra à mão: Ajustes do Sistema › Privacidade e Segurança › Gravação do Áudio do Sistema e da Tela/);
    assert.equal(v.feito, null, "a caixa disse que abriu os Ajustes que não abriram");
    await page.evaluate(() => { delete window.mac.falhar.abrir_ajustes_da_gravacao_de_tela; });

    // 5. REDEFINIR: a falha é dita e nada muda; o sucesso deixa só o reabrir.
    await page.evaluate(() => { window.mac.falhar.redefinir_permissao_de_tela = "PermissaoNaoRedefinida"; });
    await page.locator("#compartilhar-redefinir").click();
    await page.waitForFunction(() => /NÃO REDEFINIU/.test($("compartilhar-erro").textContent));
    v = await caixa();
    assert.match(v.erro, /O MACOS NÃO REDEFINIU A PERMISSÃO/);
    assert.match(v.erro, /«−».*«\+»/, "a falha de redefinir não diz o caminho à mão");
    assert.ok(v.redefinir && v.ajustes && v.passos.length === 4, `a falha de redefinir tirou as saídas: ${dito(v)}`);
    await page.evaluate(() => { delete window.mac.falhar.redefinir_permissao_de_tela; });
    await page.locator("#compartilhar-redefinir").click();
    await page.waitForFunction(() => /FOI REDEFINIDA/.test($("compartilhar-permissao-diz").textContent));
    v = await caixa();
    assert.equal(await vezes("redefinir_permissao_de_tela"), 2, "o aperto não rodou a redefinição uma vez (a primeira falhou de propósito)");
    assert.deepEqual(
      await page.evaluate(() => window.testCalls.filter((x) => x.cmd === "redefinir_permissao_de_tela").map((x) => x.args ?? null)),
      [null, null],
      "a página mandou argumento ao redefinir: o identificador é do app, e não dela",
    );
    assert.match(v.diz, /A PERMISSÃO DE GRAVAR A TELA FOI REDEFINIDA/);
    assert.match(v.nota, /Reabra o SEELE.*PEDIR AO SISTEMA.*pergunta de novo/);
    assert.ok(v.reabrir && !v.redefinir && !v.ajustes && !v.pedir && v.passos.length === 0, `depois de redefinir sobrou mais que o reabrir: ${dito(v)}`);
    assert.equal(v.erro, null);
    // E continua assim ao redesenhar e ao reabrir a caixa, sem pedir.
    await page.evaluate(() => fecharCompartilhar());
    await page.evaluate(() => abrirCompartilhar());
    v = await caixa();
    assert.ok(v.reabrir && !v.redefinir && !v.pedir, `a caixa reaberta esqueceu a redefinição: ${dito(v)}`);
    assert.equal(await vezes("pedir_permissao_de_tela"), 1, "a redefinição pediu a permissão de novo sozinha");

    // 6. REABRIR: direto sem servidor; com um no ar, pergunta antes.
    await page.locator("#compartilhar-reabrir").click();
    await page.waitForFunction(() => window.testCalls.some((x) => x.cmd === "reabrir_o_app"));
    assert.equal(await vezes("reabrir_o_app"), 1);
    await page.evaluate(() => { window.mac.servidor = { nome: "TERMINAL DOGMA", porta: 8383 }; });
    await page.locator("#compartilhar-reabrir").click();
    await page.waitForFunction(() => !$("moderar").hidden);
    v = await caixa();
    assert.equal(await vezes("reabrir_o_app"), 1, "reabriu com um servidor no ar sem perguntar");
    assert.ok(!v.aberta, "a pergunta abriu por cima da caixa de compartilhar");
    const pergunta = await page.evaluate(() => ({
      titulo: $("moderar-titulo").textContent,
      consequencia: $("moderar-consequencia").textContent,
      rotulo: $("moderar-confirmar").textContent,
    }));
    assert.equal(pergunta.titulo, "REABRIR O SEELE");
    assert.match(pergunta.consequencia, /TERMINAL DOGMA sai do ar quando o SEELE fecha, e todo mundo que estiver nele sai junto\./);
    assert.equal(pergunta.rotulo, "REABRIR E TIRAR TERMINAL DOGMA DO AR");
    await page.locator("#moderar-confirmar").click();
    await page.waitForFunction(() => window.testCalls.filter((x) => x.cmd === "reabrir_o_app").length === 2);

    // 7. Fora do macOS a negação não ganha as saídas, e concedida não diz nada.
    await page.evaluate(() => { window.mac.servidor = null; });
    await page.reload();
    await page.evaluate((r) => {
      window.testTable.snapshot = r;
      $("tela-boot").hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(r);
    }, retrato());
    await page.waitForFunction(() => $("barra-janela").dataset.plataforma === "macos");
    await page.evaluate(() => { $("barra-janela").dataset.plataforma = "windows"; window.mac.negou = true; });
    await page.evaluate(() => abrirCompartilhar());
    v = await caixa();
    assert.match(v.diz, /O SISTEMA NEGOU A GRAVAÇÃO DE TELA/);
    assert.doesNotMatch(`${v.diz} ${v.nota}`, /macOS|MACOS|Ajustes do Sistema/);
    assert.ok(!v.ajustes && !v.redefinir && !v.reabrir && !v.pedir, `fora do macOS a negação ganhou botões do macOS: ${dito(v)}`);
    await page.evaluate(() => fecharCompartilhar());
    await page.evaluate(() => { $("barra-janela").dataset.plataforma = "macos"; window.mac.concedida = true; });
    await page.evaluate(() => abrirCompartilhar());
    v = await caixa();
    assert.ok(!v.bloco && !v.pedir && !v.reabrir, `com a permissão concedida a caixa ainda fala dela: ${dito(v)}`);

    assert.deepEqual(erros, []);
    console.log(
      "gravacao-de-tela-negada: abrir não pede; o não tira o pedido e põe a versão anterior, o caminho dos Ajustes e as três saídas; " +
        "nada redesenha de volta ao pedido; os Ajustes e a redefinição só no aperto, com a falha dita; reabrir pergunta com servidor no ar.",
    );
  } finally {
    await browser.close();
    await new Promise((ok) => server.close(ok));
  }
})().catch((erro) => {
  console.error(erro);
  process.exitCode = 1;
});
