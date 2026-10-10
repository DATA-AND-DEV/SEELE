// A pergunta de quem pinta este servidor, na gestão de MODs.
//
// # Por que ela existe
//
// Tarefa 17f.0 da F3, decisão D-api-5 do dono: um tema por servidor, o
// escolhido por quem administra — como já se escolhe quem desenha cada parte
// da tela. Com dois MODs com a porta do tema ligados, só um pinta, e o
// servidor guarda qual (`seele_server::mods::tema::escolher`). Sem esta
// pergunta, quem ligou o segundo MOD de tema não tinha como saber que ele não
// pinta nada, nem como trocar: o produto sabia e não contava.
//
// # O que ela confere
//
// 1. Com dois MODs de tema ligados, a pergunta aparece, marca o que pinta
//    (`pinta: true`) e oferece o outro (`pinta: false`) — os dois valores de
//    `ModNaTela.pinta`, que é o que a orquestradora pediu como prova dele.
// 2. Escolher manda `escolher_o_tema_do_servidor` com o identificador, e o
//    desenho seguinte marca o novo.
// 3. Nenhum pinta ainda: a pergunta diz isso e oferece os dois.
// 4. Um MOD de tema só, ou sem hospedar: não há pergunta.
// 5. O segundo MOD de tema ainda no rascunho: o SALVAR avisa que vai pedir a
//    escolha, antes de a pessoa confirmar.
// 6. A recusa do servidor volta pelo nome, com a frase.
//
// A ponte do Tauri é simulada (`telas.cjs`); o Rust do comando tem os testes
// dele (`mods::tema`, no `seele-server`).
//
//   node apps/seele-app/bancada/gestao-do-tema.cjs
const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");

/** Um MOD como `mods_instalados` o manda: os campos de `ModNaTela`. */
const mod = (id, letra, extra = {}) => ({
  id,
  version: "1.0.0",
  api: 5,
  hash: letra.repeat(64),
  client: null,
  arquivos: [],
  repo: `https://github.com/${id}`,
  reach: [],
  server: true,
  tema: false,
  refused: null,
  enabled: false,
  pinta: false,
  exigencia_vale_na_rede: true,
  ...extra,
});

const ESTILO = mod("seele/estilo", "a", { tema: true, enabled: true, pinta: true });
const NOITE = mod("seele/noite", "b", { tema: true, enabled: true });
const PERFIS = mod("seele/perfis", "c", { enabled: true });

/** Desenha a gestão com esta lista, e espera o desenho terminar. */
async function desenharCom(aba, instalados, hospedando = true) {
  await aba.evaluate(async ([lista, hospeda]) => {
    window.__tabela.mods_instalados = lista;
    window.__tabela.estou_hospedando = hospeda;
    await desenharMods();
  }, [instalados, hospedando]);
}

/** O que a pergunta diz: visível ou não, a nota, e cada linha com o botão dela. */
function aPergunta(aba) {
  return aba.evaluate(() => {
    const secao = document.getElementById("mods-aparencia-do-servidor");
    if (!secao) return null;
    return {
      visivel: !secao.hidden,
      nota: secao.querySelector(".nota")?.textContent ?? "",
      linhas: [...secao.querySelectorAll("li")].map((li) => ({
        id: li.querySelector(".mods-id")?.textContent ?? "",
        texto: li.textContent,
        botao: li.querySelector("button")?.textContent ?? null,
      })),
    };
  });
}

/** A frase de porta que a linha de um MOD na lista diz, se diz alguma. */
function aLinhaDoMod(aba, id) {
  return aba.evaluate((id) => {
    const linha = [...document.querySelectorAll("#lista-mods li")]
      .find((li) => li.querySelector(".mods-id")?.textContent === id);
    return linha?.textContent ?? null;
  }, id);
}

(async () => {
  const chromium = require("./playwright.cjs").chromium();
  const s = servir();
  await new Promise((ok) => s.listen(0, "127.0.0.1", ok));
  const nav = await chromium.launch({ headless: true });
  const aba = await nav.newPage({ viewport: { width: 1280, height: 860 } });
  const erros = [];
  aba.on("pageerror", (e) => erros.push(String(e.message).slice(0, 160)));
  try {
    const r = retrato();
    await aba.addInitScript((tabela) => {
      window.__tabela = tabela;
      window.__chamadas = [];
      window.__recusa = null;
      window.__TAURI__ = {
        core: {
          invoke: async (cmd, args) => {
            window.__chamadas.push({ cmd, args });
            if (cmd === "escolher_o_tema_do_servidor") {
              if (window.__recusa) throw window.__recusa;
              // O servidor de mentira faz o que o de verdade faz: o escolhido
              // passa a ser este, e só o pacote ligado dele pinta.
              for (const m of window.__tabela.mods_instalados) {
                m.pinta = m.enabled && m.tema && m.id === args.id;
              }
              return null;
            }
            return Object.hasOwn(window.__tabela, cmd) ? window.__tabela[cmd] : null;
          },
        },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested: () => {}, isMaximized: async () => false }) },
      };
    }, respostas({
      snapshot: r,
      conjunto_exigido_agora: "conjunto-de-pe",
      servidor_hospedado: null,
      // O resto da gestão desenha junto, e uma lista nula a derruba.
      pacotes_no_cache: [],
      aceites_de_mods: [],
    }));
    await aba.goto(`http://127.0.0.1:${s.address().port}/`);
    await aba.waitForTimeout(900);

    // ------------------------------------------------ 1. a pergunta, e o `pinta`
    await desenharCom(aba, [structuredClone(ESTILO), structuredClone(NOITE), structuredClone(PERFIS)]);
    let pergunta = await aPergunta(aba);
    assert.ok(pergunta, "a gestão não tem a seção da pergunta (#mods-aparencia-do-servidor)");
    assert.equal(pergunta.visivel, true,
      "dois MODs de tema ligados e a gestão não pergunta qual pinta o servidor");
    assert.deepEqual(pergunta.linhas.map((l) => l.id), ["seele/estilo", "seele/noite"],
      "a pergunta tem de listar os dois MODs com a porta, e só eles");
    const [doEstilo, daNoite] = pergunta.linhas;
    assert.match(doEstilo.texto, /PINTA ESTE SERVIDOR/,
      "o MOD com `pinta: true` não está marcado como o que pinta");
    assert.equal(doEstilo.botao, null, "o que já pinta não pode ter botão: ele não mudaria nada");
    assert.doesNotMatch(daNoite.texto, /PINTA ESTE SERVIDOR/,
      "o MOD com `pinta: false` foi marcado como o que pinta");
    assert.equal(daNoite.botao, "PINTAR COM ESTE", "o outro MOD de tema não é oferecido");
    assert.match(pergunta.nota, /seele\/estilo pinta/, "a nota não diz quem pinta");
    assert.match(await aLinhaDoMod(aba, "seele/estilo"), /pinta a aparência deste servidor/,
      "a linha do MOD que pinta não diz isso");
    assert.match(await aLinhaDoMod(aba, "seele/noite"), /pode pintar a aparência do servidor/,
      "a linha de um MOD com a porta não diz que ele pode pintar");
    assert.doesNotMatch(await aLinhaDoMod(aba, "seele/perfis"), /aparência/,
      "um MOD sem a porta não pode dizer que pinta nada");

    // ------------------------------------------------------------ 2. escolher
    await aba.evaluate(() => {
      window.__chamadas = [];
      [...document.querySelectorAll("#mods-aparencia-do-servidor li")]
        .find((li) => li.querySelector(".mods-id")?.textContent === "seele/noite")
        .querySelector("button").click();
    });
    await aba.waitForFunction(() =>
      [...document.querySelectorAll("#mods-aparencia-do-servidor li")]
        .some((li) => li.textContent.includes("seele/noite") && li.textContent.includes("PINTA ESTE SERVIDOR")),
    null, { timeout: 3000 }).catch(() => {});
    const pedidos = await aba.evaluate(() =>
      window.__chamadas.filter((c) => c.cmd === "escolher_o_tema_do_servidor").map((c) => c.args));
    assert.deepEqual(pedidos, [{ id: "seele/noite" }], "escolher não mandou o comando com o identificador");
    pergunta = await aPergunta(aba);
    assert.match(pergunta.linhas[1].texto, /PINTA ESTE SERVIDOR/, "depois de escolher, o novo não está marcado");
    assert.equal(pergunta.linhas[0].botao, "PINTAR COM ESTE", "depois de escolher, o antigo não é oferecido");

    // --------------------------------------------------- 3. nenhum pinta ainda
    await desenharCom(aba, [
      structuredClone({ ...ESTILO, pinta: false }),
      structuredClone(NOITE),
    ]);
    pergunta = await aPergunta(aba);
    assert.equal(pergunta.visivel, true);
    assert.match(pergunta.nota, /[Nn]enhum dos dois pinta ainda/, "sem escolhido, a nota não diz que nenhum pinta");
    assert.deepEqual(pergunta.linhas.map((l) => l.botao), ["PINTAR COM ESTE", "PINTAR COM ESTE"],
      "sem escolhido, os dois têm de ser oferecidos");

    // ---------------------------------------- 4. um só, ou sem hospedar: nada
    await desenharCom(aba, [structuredClone(ESTILO), structuredClone(PERFIS)]);
    assert.equal((await aPergunta(aba)).visivel, false, "com um MOD de tema só, não há o que perguntar");
    await desenharCom(aba, [structuredClone(ESTILO), structuredClone(NOITE)], false);
    assert.equal((await aPergunta(aba)).visivel, false, "quem não hospeda não escolhe quem pinta o servidor");

    // ------------------------------------- 5. o segundo ainda no rascunho
    await desenharCom(aba, [structuredClone(ESTILO), structuredClone({ ...NOITE, enabled: false })]);
    assert.equal((await aPergunta(aba)).visivel, false,
      "o segundo MOD de tema ainda não está ligado: o servidor recusaria a escolha");
    const consequencia = await aba.evaluate(async () => {
      [...document.querySelectorAll("#lista-mods li")]
        .find((li) => li.querySelector(".mods-id")?.textContent === "seele/noite")
        .querySelector("button.botao-fantasma:not([title])").click();
      await new Promise((ok) => setTimeout(ok, 200));
      await salvarAlteracoes();
      return document.getElementById("moderar-consequencia")?.textContent ?? "";
    });
    assert.match(consequencia, /seele\/estilo e seele\/noite podem pintar a aparência deste servidor/,
      `o SALVAR não avisa que o segundo MOD de tema pede a escolha — diz «${consequencia}»`);
    await aba.evaluate(() => { document.getElementById("moderar").hidden = true; descartarORascunho(); });

    // ------------------------------------------------------------ 6. a recusa
    await desenharCom(aba, [structuredClone(ESTILO), structuredClone(NOITE)]);
    await aba.evaluate(() => {
      window.__recusa = { Recusado: { motivo: "sem-porta-de-tema" } };
      [...document.querySelectorAll("#mods-aparencia-do-servidor li button")][0].click();
    });
    await aba.waitForTimeout(300);
    const erro = await aba.evaluate(() => {
      const no = document.getElementById("mods-gestao-erro");
      return no.hidden ? "" : no.textContent;
    });
    assert.match(erro, /porta do tema/, `a recusa do servidor não chegou com a frase — diz «${erro}»`);
  } finally {
    await nav.close();
    s.close();
  }
  assert.deepEqual([...new Set(erros)], [], "a página escreveu erro");
  console.log("gestão do tema: a pergunta marca quem pinta, troca, cala sem o que perguntar, avisa no SALVAR e diz a recusa");
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
