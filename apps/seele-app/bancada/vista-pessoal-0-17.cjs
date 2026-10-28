// **A vista da ligação pessoal e a entrada com SEU CÓDIGO e AMIGOS** (0.17),
// medidas no DOM RENDERIZADO, em Chromium ou WebKit.
//
// # O que é simulado, e o que é o produto
//
// SIMULADO: a ponte (na parte 1, a `ponte-web.js` de verdade com duas sessões
// de mentira postas pela costura `sessaoDaPonte.atual = …`; na parte 2, um
// `window.__TAURI__` com respostas combinadas), as sessões, o retrato da sala
// e os eventos. Nenhum áudio, nenhuma rede, nenhum Rust.
// REAL: o `index.html`, todo o CSS e todo o JS de `ui/` — o que se mede é o
// layout que o produto desenha.
//
// # O que ela prova (parte 1, a casca larga, 1920×1080)
//
// 1. Duas sessões: um servidor e uma ligação pessoal (`ligacao` != null).
// 2. Na vista pessoal ficam VISÍVEIS, dentro da janela e com caixa > 0: a grade
//    de participantes, o palco da tela, o microfone, AJUSTES e SAIR DA SALA; e a
//    trilha das sessões. Ficam FORA: criar sala, canais de texto e a conversa.
// 3. Trocar para o servidor mostra a vista normal; voltar mostra a pessoal.
// 4. A troca de vista NÃO chama foco_de_voz, focar_voz, disconnect, sair da
//    sala nem encerrar sessão; e nenhum controle aparece duplicado.
//
// Parte 2 (Tauri simulado): a entrada mostra SEU CÓDIGO com COPIAR, a frase
// honesta da sala e a lista de AMIGOS com LIGAR, tudo visível na janela.
//
// Rodar:  node apps/seele-app/bancada/vista-pessoal-0-17.cjs [--navegador=webkit]
//         SEELE_CAPTURAS=<pasta> muda onde as capturas vão.

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { servir, respostas, retrato } = require("./telas.cjs");
const pw = require("./playwright.cjs");

const navegador = (process.argv.find((a) => a.startsWith("--navegador="))?.slice(12)) || "chromium";
const motor = navegador === "webkit" ? pw.webkit() : pw.chromium();
const capturas = process.argv.find((a) => a.startsWith("--capturas="))?.slice(11) ?? process.env.SEELE_CAPTURAS ??path.join(require("node:os").tmpdir(), "seele-capturas-vista-pessoal");
fs.mkdirSync(capturas, { recursive: true });
const janela = (process.argv.find((a) => a.startsWith("--janela="))?.slice(9) ?? "1920x1080").split("x").map(Number);
const VIEWPORT = { width: janela[0], height: janela[1] };
const tam = `${VIEWPORT.width}x${VIEWPORT.height}`;

/** Corre no navegador: visível = sem `hidden`/display/visibility, caixa > 0, dentro da janela. */
const MEDIDOR = `
  window.__medir = (seletor) => {
    const achados = [...document.querySelectorAll(seletor)];
    return achados.map((el) => {
      let visivel = true;
      for (let n = el; n && n.nodeType === 1; n = n.parentElement) {
        const e = getComputedStyle(n);
        if (n.hidden || e.display === "none" || e.visibility === "hidden") { visivel = false; break; }
      }
      const r = el.getBoundingClientRect();
      const dentro = r.width > 0 && r.height > 0 && r.right > 0 && r.bottom > 0 &&
        r.left < innerWidth && r.top < innerHeight;
      return visivel && dentro;
    });
  };
  window.__visivel = (seletor) => window.__medir(seletor).some(Boolean);
`;

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const url = `http://127.0.0.1:${server.address().port}/`;
  const browser = await motor.launch({ headless: true });
  const erros = [];
  try {
    // ------------------------------------------------ parte 1: a ponte web
    const page = await browser.newPage({ viewport: VIEWPORT });
    page.on("pageerror", (e) => erros.push(e.message));
    await page.addInitScript(() => {
      try { localStorage.setItem("seele.casca", "larga"); } catch {}
    });
    await page.goto(url);
    await page.waitForTimeout(500);
    await page.evaluate(MEDIDOR);

    await page.evaluate(async (base) => {
      window.anotado = [];
      const falsa = (nome, extra, outros) => {
        const sala = {
          ...base,
          server: nome,
          voice_rooms: [{
            id: 1, name: extra.sala, limit: 8, password_required: false, occupied_by_us: true, channel: 1,
            people: [
              { id: 1, nickname: "Alex", speaking: false, muted: false, total_isolation: false, signal: 92, sync_band: "Nominal", is_self: true },
              ...outros,
            ],
            sync: null,
          }],
        };
        const s = {
          alvo: { endereco: extra.endereco, impressao: nome },
          acabou: false,
          ligacao: extra.ligacao ?? null,
          retrato: () => ({ sala, contados: {}, telemetria: { rtt_ms: 10, jitter_ms: 1, loss_fraction: 0, signal: 90, sync_band: "Nominal" } }),
          enlace: () => "Online",
          atender: (comando, args) => {
            anotado.push({ sessao: nome, comando, args });
            if (comando === "leave_voice_room" && !window.falharSaida) sala.voice_rooms[0].occupied_by_us = false;
            if (comando === "leave_voice_room" && window.falharSaida) return { erro: "SaidaRecusada" };
            if (comando === "enter_voice_room" && window.falharEntrada) return { erro: "SalaCheia" };
            if (comando === "enter_voice_room") sala.voice_rooms[0].occupied_by_us = true;
            // Sem áudio de verdade na bancada: a ponte confere a voz e a fecha.
            if (comando === "sala_da_voz") return { resposta: { sala: null, voz: 0 } };
            if (comando === "messages") return { resposta: [] };
            return { resposta: null };
          },
          encerrar: async () => { s.acabou = true; anotado.push({ sessao: nome, comando: "encerrar" }); },
        };
        s.snapshot = () => retratoDaSessao(s);
        return s;
      };
      const pessoa = (id, nickname) => ({ id, nickname, speaking: false, muted: false, total_isolation: false, signal: 80, sync_band: "Nominal", is_self: false });
      sessaoDaPonte.atual = falsa("Casa do Lago", { sala: "Sala principal", endereco: "casa.exemplo:8383" }, [pessoa(2, "Lia")]);
      sessaoDaPonte.atual = falsa("Ana", { sala: "Ligação", endereco: "@K9P2-CSM7-NBFR-AYQ3", ligacao: "K9P2-CSM7-NBFR-AYQ3" }, [pessoa(2, "Ana")]);
      window.fabricar = { falsa, pessoa };
      // Registra o que passa pela ponte, ao lado do que as sessões recebem.
      window.passou = [];
      // `invoke` é o global do produto (base.js): é por ele que passa todo clique da tela.
      const original = window.invoke;
      window.invoke = async (comando, args) => {
        passou.push(comando);
        if (comando === "snapshot" && window.__atrasar) await new Promise((r) => setTimeout(r, 900));
        return original(comando, args);
      };
      const entrada = await invoke("ver_sessao", { sessao: 2 });
      for (const t of document.querySelectorAll("section.tela")) t.hidden = true;
      $("tela-sessao").hidden = false;
      desenhar(entrada.snapshot);
      await recarregarSessoes();
    }, retrato());
    await page.waitForTimeout(600);

    const medir = (seletor) => page.evaluate((s) => window.__medir(s), seletor);
    const visivel = (seletor) => page.evaluate((s) => window.__visivel(s), seletor);

    // 2. A vista pessoal.
    const lista = await page.evaluate(() => sessoesNaTela.map((s) => [s.carimbo.sessao, s.nome, s.na_vista, s.ligacao]));
    assert.deepEqual(lista, [[1, "Casa do Lago", false, null], [2, "Ana", true, "K9P2-CSM7-NBFR-AYQ3"]],
      `a ponte não lista o servidor e a ligação: ${JSON.stringify(lista)}`);
    assert.equal(await page.evaluate(() => $("tela-sessao").dataset.vista), "ligacao", "a vista não é a pessoal");

    const DA_CHAMADA = {
      "a grade de participantes": "#chamada-grade",
      "o palco da tela": "#palco",
      "o microfone": "#botao-mudo",
      "AJUSTES (dispositivos)": "#botao-server-sessao",
      "SAIR DA SALA": "#operador-sair",
      "a trilha das sessões": "#trilha-sessoes",
      "a faixa da ligação pessoal": "#sessao-ligacao",
    };
    const FORA = {
      "criar sala de voz": "#nova-voice_room",
      "os canais de texto": "#lista-linhas",
      "o cabeçalho dos canais": ".canais-cabeca-linhas",
      "a conversa": "#vista-conversa",
    };
    for (const [quem, seletor] of Object.entries(DA_CHAMADA)) {
      assert.ok(await visivel(seletor), `na vista pessoal ${quem} (${seletor}) não está visível dentro da janela`);
    }
    for (const [quem, seletor] of Object.entries(FORA)) {
      assert.ok(!(await visivel(seletor)), `na vista pessoal ${quem} (${seletor}) ainda aparece`);
    }
    for (const [quem, seletor] of Object.entries({
      "o roster do servidor": "#lista-roster", "o painel de pessoas": ".painel-pessoas",
      "a lista de salas de voz": "#lista-voice_rooms", "a coluna de salas": ".canais-rolagem",
    })) assert.ok(!(await visivel(seletor)), `na vista pessoal ${quem} (${seletor}) ainda aparece`);
    const textos = await page.evaluate(() => {
      const achados = [];
      const andar = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
      for (let n = andar.nextNode(); n; n = andar.nextNode()) {
        if (!/NO SERVIDOR|SALAS DE VOZ|dois|só entre/i.test(n.textContent)) continue;
        const el = n.parentElement;
        if (!el || el.closest("[hidden]")) continue;
        let oculto = false;
        for (let x = el; x; x = x.parentElement) { const e = getComputedStyle(x); if (e.display === "none" || e.visibility === "hidden") { oculto = true; break; } }
        if (!oculto && el.getBoundingClientRect().width > 0) achados.push(n.textContent.trim().slice(0, 60));
      }
      return achados;
    });
    assert.deepEqual(textos, [], `a vista pessoal escreve texto de servidor ou promessa falsa: ${textos}`);
    const quantasAna = await page.evaluate(() => [...document.querySelectorAll(".chamada-cartao")].filter((c) => c.dataset.pessoa === "Ana" && c.getClientRects().length > 0).length);
    assert.equal(quantasAna, 1, `a participante aparece ${quantasAna} vezes na grade`);
    assert.match(await page.textContent("#sessao-ligacao"), /^LIGAÇÃO PESSOAL · Ana$/, "o rótulo da ligação não é LIGAÇÃO PESSOAL e o nome");
    const semRolagemLateral = await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1);
    assert.ok(semRolagemLateral, "a vista pessoal rola na horizontal");
    const cartoes = await page.evaluate(() => document.querySelectorAll("#chamada-grade > *").length);
    assert.ok(cartoes >= 2, `a grade não mostra os participantes da ligação: ${cartoes}`);
    assert.equal(await page.evaluate(() => $("trilha-sessoes").children.length), 2, "a trilha não lista as duas sessões");
    await page.screenshot({ path: path.join(capturas, `vista-pessoal-${navegador}-${tam}.png`) });

    // 2b. O miolo nunca fica vazio, e «fora da sala» só depois de uma saída
    //     CONFIRMADA. Teclado e cliques reais; visibilidade computada.
    const algumPainel = async () => (await visivel("#vista-chamada")) || (await visivel("#vista-ligacao-fora"));
    const comandosDesde = (marca) => page.evaluate((m) => passou.slice(m), marca);
    const tamanho = () => page.evaluate(() => passou.length);
    const foraDaSala = async (como, esperaEntrar) => {
      assert.ok(await algumPainel(), `${como}: os dois painéis do miolo ficaram invisíveis`);
      assert.ok(!(await visivel("#vista-chamada")), `${como}: a grade continuou aberta`);
      assert.ok(await visivel("#vista-ligacao-fora"), `${como}: o painel fora da sala não apareceu`);
      for (const [quem, seletor] of Object.entries({
        "o estado da ligação": "#ligacao-fora-estado",
        "DESLIGAR": "#ligacao-encerrar", "VOLTAR": "#ligacao-voltar",
        [esperaEntrar ? "ENTRAR NA SALA" : "VOLTAR À CHAMADA"]: esperaEntrar ? "#ligacao-entrar" : "#ligacao-reabrir",
      })) assert.ok(await visivel(seletor), `${como}: ${quem} não está visível`);
      assert.ok(!(await visivel(esperaEntrar ? "#ligacao-reabrir" : "#ligacao-entrar")),
        `${como}: oferece ${esperaEntrar ? "VOLTAR À CHAMADA" : "ENTRAR NA SALA"}, que não é o caso`);
      assert.ok(!(await visivel("#vista-conversa")), `${como}: a conversa de servidor apareceu na ligação`);
    };
    const estadoDoFora = () => page.textContent("#ligacao-fora-estado");

    // (a) Voz ativa: o Escape só fecha sobreposições. A grade fica, a voz fica.
    let marca = await tamanho();
    const focoAntes = await page.evaluate(() => focoDaPonte);
    await page.keyboard.press("Escape");
    await page.waitForTimeout(300);
    assert.ok(await visivel("#chamada-grade") && await visivel("#vista-chamada"), "(a) o Escape fechou a grade da ligação");
    assert.ok(!(await visivel("#vista-ligacao-fora")), "(a) apareceu «fora da sala» com a voz ativa");
    const doEscape = await comandosDesde(marca);
    assert.ok(!doEscape.includes("leave_voice_room") && !doEscape.includes("focar_voz"), `(a) o Escape mexeu na voz: ${doEscape}`);
    assert.equal(await page.evaluate(() => focoDaPonte), focoAntes, "(a) o Escape mudou o foco de voz");
    assert.equal(await page.evaluate(() => document.querySelector("#tela-sessao").dataset.vista), "ligacao");
    await page.screenshot({ path: path.join(capturas, `escape-mantem-a-grade-${navegador}-${tam}.png`) });

    // (b) A saída FALHA: a grade fecha mas a pessoa continua sentada. O painel
    //     diz a ocupação real e oferece VOLTAR À CHAMADA, não ENTRAR.
    await page.evaluate(() => { window.falharSaida = true; });
    marca = await tamanho();
    await page.click("#operador-sair");
    await page.waitForTimeout(600);
    assert.ok((await comandosDesde(marca)).includes("leave_voice_room"), "(b) SAIR DA SALA não pediu o leave");
    await foraDaSala("(b) saída que falhou", false);
    assert.match(await estadoDoFora(), /continua na sala/, "(b) o painel diz que saiu, e a pessoa continua sentada");
    await page.screenshot({ path: path.join(capturas, `saida-falhou-continua-na-sala-${navegador}-${tam}.png`) });
    await page.click("#ligacao-reabrir");
    await page.waitForTimeout(500);
    assert.ok(await visivel("#chamada-grade"), "(b) VOLTAR À CHAMADA não reabriu a grade");
    assert.ok(!(await visivel("#vista-ligacao-fora")), "(b) o painel ficou sobre a grade");

    // (b2) A saída CONFIRMADA: o painel «fora da sala» com ENTRAR.
    await page.evaluate(() => { window.falharSaida = false; });
    await page.click("#operador-sair");
    await page.waitForTimeout(600);
    await foraDaSala("(b2) saída confirmada", true);
    assert.match(await estadoDoFora(), /Você está fora da sala da ligação com Ana\. A ligação continua aberta/, "(b2) o painel não diz o estado neutro");
    await page.screenshot({ path: path.join(capturas, `fora-da-sala-confirmado-${navegador}-${tam}.png`) });
    await page.keyboard.press("Escape");
    await page.waitForTimeout(250);
    assert.ok(await algumPainel(), "Escape deixou os dois painéis invisíveis");

    // (c) A entrada FALHA: a frase aparece na vista e a grade NÃO abre.
    await page.evaluate(() => { window.falharEntrada = true; });
    marca = await tamanho();
    await page.click("#ligacao-entrar");
    await page.waitForTimeout(600);
    assert.ok((await comandosDesde(marca)).includes("enter_voice_room"), "(c) ENTRAR não pediu a entrada");
    assert.ok(await visivel("#ligacao-fora-erro"), "(c) a falha da entrada não aparece na vista");
    const fraseEsperada = await page.evaluate(() => fraseDeErro("SalaCheia"));
    assert.equal((await page.textContent("#ligacao-fora-erro")).trim(), fraseEsperada.trim(), "(c) a frase da recusa não é a do produto");
    assert.ok(fraseEsperada.trim().length > 0, "(c) a recusa ficou sem frase");
    assert.ok(!(await visivel("#vista-chamada")), "(c) a grade abriu apesar da entrada ter falhado");
    assert.ok(await visivel("#ligacao-entrar"), "(c) sumiu o ENTRAR para tentar de novo");
    await page.screenshot({ path: path.join(capturas, `entrada-falhou-${navegador}-${tam}.png`) });

    // (d) A entrada dá certo: a grade abre e a falha some.
    await page.evaluate(() => { window.falharEntrada = false; });
    await page.click("#ligacao-entrar");
    await page.waitForTimeout(700);
    assert.ok(await visivel("#chamada-grade") && await visivel("#vista-chamada"),
      `(d) a entrada deu certo e a grade não abriu: erro=${await page.textContent("#ligacao-fora-erro")} comandos=${await comandosDesde(marca)} erros=${erros}`);
    assert.ok(!(await visivel("#vista-ligacao-fora")) && !(await visivel("#ligacao-fora-erro")), "(d) o painel ou a falha ficaram");

    // 3. Trocar para o servidor (pelo clique na trilha) e voltar.
    await page.evaluate(() => { anotado.length = 0; passou.length = 0; });
    await page.click('#trilha-sessoes [data-sessao="1"]');
    await page.waitForFunction(() => $("tela-sessao").dataset.vista === "servidor");
    await page.waitForTimeout(500);
    assert.ok(await visivel("#vista-conversa"), "no servidor a conversa não voltou");
    assert.ok(await visivel("#lista-linhas") && await visivel("#nova-voice_room"), "no servidor os canais e criar sala não voltaram");
    assert.ok(!(await visivel("#sessao-ligacao")), "a faixa da ligação ficou no servidor");
    assert.ok(await visivel("#trilha-sessoes"), "a trilha sumiu no servidor");
    await page.screenshot({ path: path.join(capturas, `vista-servidor-${navegador}-${tam}.png`) });

    await page.click('#trilha-sessoes [data-sessao="2"]');
    await page.waitForFunction(() => $("tela-sessao").dataset.vista === "ligacao");
    await page.waitForTimeout(500);
    for (const [quem, seletor] of Object.entries(DA_CHAMADA)) {
      assert.ok(await visivel(seletor), `ao voltar à pessoal ${quem} (${seletor}) sumiu`);
    }
    assert.ok(!(await visivel("#vista-conversa")), "ao voltar à pessoal a conversa reapareceu");

    // 4. Comandos da troca, e nenhum controle duplicado.
    const comandos = await page.evaluate(() => passou.slice());
    const proibidos = ["foco_de_voz", "focar_voz", "disconnect", "encerrar_sessao", "leave_voice_room"];
    const pegos = comandos.filter((c) => proibidos.includes(c));
    assert.deepEqual(pegos, [], `a troca de vista chamou ${pegos} (todos: ${comandos})`);
    const dasSessoes = await page.evaluate(() => anotado.map((a) => `${a.sessao}:${a.comando}`));
    assert.ok(!dasSessoes.some((c) => /leave_voice_room|encerrar|disconnect/.test(c)),
      `a troca mexeu numa sessão: ${dasSessoes}`);
    assert.ok(comandos.includes("ver_sessao"), `a troca não passou por ver_sessao: ${comandos}`);

    const ids = await page.evaluate(() => {
      const vistos = new Map();
      for (const el of document.querySelectorAll("[id]")) vistos.set(el.id, (vistos.get(el.id) ?? 0) + 1);
      return [...vistos].filter(([, n]) => n > 1).map(([id]) => id);
    });
    assert.deepEqual(ids, [], `ids duplicados na página renderizada: ${ids}`);
    for (const seletor of ["#botao-mudo", "#operador-sair", "#botao-server-sessao", "#chamada-grade", "#palco", "#trilha-adicionar"]) {
      const n = (await medir(seletor)).filter(Boolean).length;
      assert.equal(n, 1, `${seletor} aparece ${n} vezes na tela`);
    }
    const sairVisiveis = await page.evaluate(() =>
      [...document.querySelectorAll("button")].filter((b) => /^SAIR DA SALA$/.test(b.textContent.trim()) && window.__medir && b.getClientRects().length > 0).length);
    assert.equal(sairVisiveis, 1, `SAIR DA SALA aparece ${sairVisiveis} vezes`);

    // 5. A corrida: abrir a vista pessoal e trocar logo para o servidor, com a
    //    resposta do retrato atrasada. O servidor fica correto e sem a grade da
    //    ligação; voltar à pessoal mostra a grade (ou o fora da sala), nunca vazio.
    await page.click('#trilha-sessoes [data-sessao="1"]');
    await page.waitForFunction(() => $("tela-sessao").dataset.vista === "servidor");
    await page.waitForTimeout(400);
    await page.evaluate(() => {
      window.__atrasar = true;
    });
    await page.click('#trilha-sessoes [data-sessao="2"]');
    await page.waitForFunction(() => $("tela-sessao").dataset.vista === "ligacao");
    await page.click('#trilha-sessoes [data-sessao="1"]');
    await page.waitForFunction(() => $("tela-sessao").dataset.vista === "servidor");
    await page.waitForTimeout(1600);
    assert.ok(await visivel("#vista-conversa"), "corrida: a vista do servidor ficou sem a conversa");
    assert.ok(!(await visivel("#vista-chamada")) && !(await visivel("#chamada-grade")), "corrida: a grade da ligação ficou na vista do servidor");
    assert.ok(!(await visivel("#vista-ligacao-fora")) && !(await visivel("#sessao-ligacao")), "corrida: sobrou algo da ligação no servidor");
    await page.screenshot({ path: path.join(capturas, `corrida-servidor-${navegador}-${tam}.png`) });
    await page.evaluate(() => { window.__atrasar = false; });
    await page.click('#trilha-sessoes [data-sessao="2"]');
    await page.waitForFunction(() => $("tela-sessao").dataset.vista === "ligacao");
    await page.waitForTimeout(700);
    assert.ok(await algumPainel(), "corrida: voltar à pessoal deixou o miolo vazio");
    assert.ok(await visivel("#chamada-grade"), "corrida: voltar à pessoal não mostra a grade");
    await page.screenshot({ path: path.join(capturas, `corrida-volta-pessoal-${navegador}-${tam}.png`) });

    // 6. DESLIGAR / SAIR DESTA LIGAÇÃO saem SÓ desta sessão. O servidor aberto
    //    continua; a sala pessoal desta máquina (que o navegador não tem, e aqui
    //    é só o comando `encerrar_a_ligacao`, que nunca pode passar) não é tocada.
    const saidas = () => page.evaluate(() => ({
      passou: passou.slice(),
      anotado: anotado.map((a) => `${a.sessao}:${a.comando}`),
      sessoes: sessoesNaTela.map((s) => s.nome),
    }));
    // (a) quem LIGOU.
    await page.evaluate(() => { passou.length = 0; anotado.length = 0; });
    await page.click("#operador-sair");
    await page.waitForTimeout(600);
    assert.equal((await page.textContent("#ligacao-encerrar")).trim(), "DESLIGAR", "quem ligou não vê DESLIGAR");
    await page.click("#ligacao-encerrar");
    await page.waitForTimeout(700);
    let s = await saidas();
    assert.ok(!s.passou.includes("encerrar_a_ligacao"), `DESLIGAR fechou a sala pessoal: ${s.passou}`);
    assert.ok(s.passou.includes("encerrar_sessao"), `DESLIGAR não saiu da sessão: ${s.passou}`);
    assert.deepEqual(s.anotado.filter((c) => /encerrar/.test(c)), ["Ana:encerrar"], `DESLIGAR tocou noutra sessão: ${s.anotado}`);
    assert.deepEqual(s.sessoes, ["Casa do Lago"], `o servidor não continua aberto sozinho: ${s.sessoes}`);
    assert.ok(await visivel("#boot-sessoes"), "depois de desligar a entrada não lista o servidor que continua aberto");
    await page.screenshot({ path: path.join(capturas, `depois-de-desligar-${navegador}-${tam}.png`) });
    // (b) quem RECEBEU, na minha sala: o rótulo diz «sair», não «encerrar para todos».
    await page.evaluate(async () => {
      const { falsa, pessoa } = window.fabricar;
      const d = falsa("Beto", { sala: "Minha sala", endereco: "@MINHA-SALA" }, [pessoa(2, "Beto")]);
      d.onde = "Pessoal";
      sessaoDaPonte.atual = d;
      await recarregarSessoes();
      await verSessao(sessoesNaTela.find((x) => x.nome === "Beto").carimbo.sessao);
      passou.length = 0; anotado.length = 0;
    });
    await page.waitForFunction(() => $("tela-sessao").dataset.vista === "ligacao");
    await page.waitForTimeout(500);
    await page.click("#operador-sair");
    await page.waitForTimeout(600);
    assert.equal((await page.textContent("#ligacao-encerrar")).trim(), "SAIR DESTA LIGAÇÃO", "quem recebeu não vê SAIR DESTA LIGAÇÃO");
    assert.match(await page.textContent("#ligacao-fora-estado"), /sua sala pessoal continuam/,
      `o estado não diz o que o botão faz: ${JSON.stringify(await page.evaluate(() => ({ passou: passou.slice(), fora: !$("vista-ligacao-fora").hidden, grade: !$("vista-chamada").hidden, leituras: leiturasDoFora, ep: epocaDaVista })))}`);
    await page.screenshot({ path: path.join(capturas, `recebida-fora-da-sala-${navegador}-${tam}.png`) });
    await page.click("#ligacao-encerrar");
    await page.waitForTimeout(700);
    s = await saidas();
    assert.ok(!s.passou.includes("encerrar_a_ligacao"), `SAIR DESTA LIGAÇÃO encerrou para todos: ${s.passou}`);
    assert.deepEqual(s.anotado.filter((c) => /encerrar/.test(c)), ["Beto:encerrar"], `a saída tocou noutra sessão: ${s.anotado}`);
    assert.deepEqual(s.sessoes, ["Casa do Lago"], `as outras sessões não continuam: ${s.sessoes}`);

    // 7. O cenário do CUA: a ligação recém-conectada, com o retrato FORA da sala.
    //    A vista decide pela ocupação real: painel de fora com ENTRAR e DESLIGAR,
    //    sem a grade vazia, sem entrar sozinha e sem mexer no foco de voz.
    const nova = async (nome, codigo, dentro = false) => {
      await page.evaluate(async ([nome, codigo, dentro]) => {
        const { falsa, pessoa } = window.fabricar;
        const d = falsa(nome, { sala: "Ligação", endereco: `@${codigo}`, ligacao: codigo }, [pessoa(2, nome)]);
        d.retrato().sala.voice_rooms[0].occupied_by_us = dentro;
        sessaoDaPonte.atual = d;
        // Sem reler as sessões aqui: o estado é o de uma ligação que acaba de
        // conectar, e quem a mostra é o `verSessao` real, logo depois.
        window.__nova = [...sessoesDaPonte.values()].find((x) => x.alvo.impressao === nome).idDaPonte;
      }, [nome, codigo, dentro]);
    };
    await nova("Carla", "53SQ-AAAA-BBBB-CCCC");
    await page.evaluate(() => { passou.length = 0; anotado.length = 0; window.__foco = focoDaPonte; });
    await page.evaluate(() => verSessao(window.__nova));
    await page.waitForFunction(() => $("tela-sessao").dataset.vista === "ligacao");
    await page.waitForTimeout(800);
    assert.ok(await visivel("#vista-ligacao-fora"), "(a) a ligação fora da sala não mostra o painel de fora");
    assert.ok(await visivel("#ligacao-entrar"), "(a) falta ENTRAR NA SALA");
    assert.ok(await visivel("#ligacao-encerrar"), "(a) falta DESLIGAR");
    assert.ok(!(await visivel("#vista-chamada")), "(a) a grade vazia apareceu com a pessoa fora da sala");
    const primeiro = await page.textContent("#ligacao-fora-estado");
    assert.ok(/Você está fora da sala da ligação com Carla\./.test(primeiro) && !/saiu/i.test(primeiro), `(a) o primeiro acesso não diz o estado neutro: ${primeiro}`);
    assert.ok(!(await visivel("#ligacao-reabrir")), "(a) ofereceu VOLTAR À CHAMADA a quem não está na sala");
    const inicial = await page.evaluate(() => ({ passou: passou.slice(), foco: focoDaPonte, antes: window.__foco, nomes: sessoesNaTela.map((x) => x.nome) }));
    assert.ok(!inicial.passou.includes("enter_voice_room"), `(a) entrou na sala sozinha: ${inicial.passou}`);
    assert.equal(inicial.foco, inicial.antes, "(a) o foco de voz mudou sem ação da pessoa");
    assert.ok(inicial.nomes.includes("Casa do Lago"), `(a) a sessão A não continua: ${inicial.nomes}`);
    await page.screenshot({ path: path.join(capturas, `ligacao-recem-conectada-fora-${navegador}-${tam}.png`) });
    // (b) ENTRAR: a falha aparece e a grade não abre; com sucesso a grade abre.
    await page.evaluate(() => { window.falharEntrada = true; });
    await page.click("#ligacao-entrar");
    await page.waitForTimeout(600);
    assert.ok(await visivel("#ligacao-fora-erro") && !(await visivel("#vista-chamada")), "(b) a falha do ENTRAR não aparece, ou a grade abriu");
    await page.evaluate(() => { window.falharEntrada = false; });
    await page.click("#ligacao-entrar");
    await page.waitForTimeout(700);
    assert.ok((await page.evaluate(() => passou)).includes("enter_voice_room"), "(b) ENTRAR não chamou enter_voice_room");
    assert.ok(await visivel("#chamada-grade") && !(await visivel("#vista-ligacao-fora")), "(b) a entrada deu certo e a grade não abriu");
    // (c) DESLIGAR sai só desta sessão.
    await nova("Dora", "7DDD-AAAA-BBBB-CCCC");
    await page.evaluate(() => { passou.length = 0; anotado.length = 0; });
    await page.evaluate(() => verSessao(window.__nova));
    await page.waitForFunction(() => $("tela-sessao").dataset.vista === "ligacao");
    await page.waitForTimeout(800);
    assert.ok(await visivel("#ligacao-encerrar"), "(c) falta DESLIGAR");
    await page.click("#ligacao-encerrar");
    await page.waitForTimeout(700);
    s = await saidas();
    assert.ok(!s.passou.includes("encerrar_a_ligacao"), `(c) DESLIGAR fechou a sala pessoal: ${s.passou}`);
    assert.deepEqual(s.anotado.filter((c) => /encerrar/.test(c)), ["Dora:encerrar"], `(c) DESLIGAR tocou noutra sessão: ${s.anotado}`);
    assert.ok(s.sessoes.includes("Casa do Lago") && s.sessoes.includes("Carla") && !s.sessoes.includes("Dora"), `(c) as outras sessões não continuam: ${s.sessoes}`);

    // 8. A leitura de «fora da sala» da sessão A fica atrasada; a pessoa troca
    //    para B; a resposta de A chega: a vista B não mostra o estado de A e
    //    não fica em «Conferindo…».
    await nova("Eva", "9EEE-AAAA-BBBB-CCCC");
    await page.evaluate(() => { window.__eva = window.__nova; sessoesDaPonte.get(window.__nova).retrato().sala.voice_rooms[0].occupied_by_us = true; });
    await page.evaluate(() => verSessao(window.__nova));
    await page.waitForFunction(() => $("tela-sessao").dataset.vista === "ligacao");
    await page.waitForTimeout(700);
    assert.ok(await visivel("#chamada-grade"), "(8) a ligação de Eva, na sala, não abriu a grade");
    await page.evaluate(() => {
      window.falharSaida = true;
      window.__sessaoAtrasada = vistaDaJanela.sessao;
      const original = window.invoke;
      window.invoke = async (comando, args) => {
        if (comando === "snapshot" && vistaDaJanela.sessao === window.__sessaoAtrasada) await new Promise((r) => setTimeout(r, 1500));
        return original(comando, args);
      };
    });
    await page.click("#operador-sair");
    await page.waitForTimeout(200);
    assert.match(await page.textContent("#ligacao-fora-estado"), /Conferindo/, "(8) a leitura de Eva não ficou pendente");
    await nova("Fabi", "8FFF-AAAA-BBBB-CCCC");
    await page.evaluate(() => { window.__fabi = window.__nova; });
    await page.evaluate(() => verSessao(window.__nova));
    await page.waitForFunction(() => vistaPessoalAberta === window.__nova);
    await page.waitForTimeout(500);
    const deB = await page.textContent("#ligacao-fora-estado");
    assert.ok(/fora da sala da ligação com Fabi/.test(deB), `(8) a vista de Fabi não mostra o estado dela: ${deB}`);
    await page.waitForTimeout(1600);
    const depois = await page.textContent("#ligacao-fora-estado");
    assert.ok(/fora da sala da ligação com Fabi/.test(depois) && !/Eva|Conferindo/.test(depois), `(8) a resposta atrasada de Eva pintou a vista de Fabi, ou travou: ${depois}`);
    assert.ok(await visivel("#ligacao-entrar") && !(await visivel("#ligacao-reabrir")), "(8) a vista de Fabi ficou sem ENTRAR, ou com o botão de Eva");
    await page.screenshot({ path: path.join(capturas, `leitura-atrasada-de-outra-sessao-${navegador}-${tam}.png`) });
    await page.evaluate(() => { window.falharSaida = false; window.__sessaoAtrasada = null; });

    // 9. A→B→A: a resposta atrasada da PRIMEIRA visita a Eva chega na SEGUNDA e
    //    é descartada; a segunda mostra o estado da leitura dela (fora da sala).
    await page.evaluate(() => verSessao(window.__eva));
    await page.waitForFunction(() => vistaPessoalAberta === window.__eva);
    await page.waitForTimeout(700);
    await page.evaluate(() => {
      sessoesDaPonte.get(window.__eva).retrato().sala.voice_rooms[0].occupied_by_us = true;
      window.falharSaida = true;
      window.__atrasarEsta = true;
      const original = window.invoke;
      // A resposta é amostrada no pedido e ENTREGUE tarde: o estado dela é o do
      // momento em que a primeira visita a perguntou.
      window.invoke = async (comando, args) => {
        if (comando === "snapshot" && window.__atrasarEsta) {
          window.__atrasarEsta = false;
          const resposta = await original(comando, args);
          await new Promise((r) => setTimeout(r, 1800));
          return resposta;
        }
        return original(comando, args);
      };
    });
    await page.click("#operador-sair");
    await page.waitForTimeout(250);
    await page.evaluate(() => {
      window.falharSaida = false;
      sessoesDaPonte.get(window.__eva).retrato().sala.voice_rooms[0].occupied_by_us = false;
    });
    await page.evaluate(() => verSessao(window.__fabi));
    await page.waitForFunction(() => vistaPessoalAberta === window.__fabi);
    await page.evaluate(() => verSessao(window.__eva));
    await page.waitForFunction(() => vistaPessoalAberta === window.__eva);
    await page.waitForTimeout(500);
    const segunda = await page.textContent("#ligacao-fora-estado");
    assert.ok(/fora da sala da ligação com Eva/.test(segunda), `(9) a segunda visita não mostra a leitura dela: ${segunda}`);
    await page.waitForTimeout(1800);
    const final = await page.textContent("#ligacao-fora-estado");
    assert.ok(/fora da sala da ligação com Eva/.test(final) && !/continua na sala|Conferindo/.test(final), `(9) a resposta da primeira visita foi aceita na segunda, ou travou: ${final}`);
    assert.ok(await visivel("#ligacao-entrar") && !(await visivel("#ligacao-reabrir")), "(9) a segunda visita ficou sem ENTRAR, ou com o botão da primeira");
    await page.screenshot({ path: path.join(capturas, `a-b-a-resposta-velha-descartada-${navegador}-${tam}.png`) });

    // 10. A organização v2 da ligação: sem a coluna intermediária SÓ nesta vista,
    //     a barra com os controles reais, o chat sobreposto acessível, o foco de
    //     voz parado, e o celular.
    await nova("Gabi", "6GGG-AAAA-BBBB-CCCC", true);
    await page.evaluate(() => { window.__gabi = window.__nova; });
    // O servidor primeiro: a conversa dele é a de sempre.
    await page.evaluate(() => verSessao(1));
    await page.waitForFunction(() => $("tela-sessao").dataset.vista === "servidor");
    await page.waitForTimeout(500);
    for (const [quem, seletor] of Object.entries({
      "criar sala de voz": "#nova-voice_room", "o perfil do operador": "#operador-quem",
      "a lista de salas": "#lista-voice_rooms", "a conversa": "#vista-conversa", "o campo de mensagem": "#campo-mensagem",
    })) assert.ok(await visivel(seletor), `(10) no servidor ${quem} (${seletor}) não está visível`);
    for (const seletor of ["#ligacao-chat", "#ligacao-desligar", "#ligacao-cabeca"]) {
      assert.ok(!(await visivel(seletor)), `(10) no servidor apareceu ${seletor}`);
    }
    const dialogoNoServidor = await page.evaluate(() => ({ papel: $("vista-conversa").getAttribute("role"), chat: $("tela-sessao").dataset.chat ?? null }));
    assert.deepEqual(dialogoNoServidor, { papel: null, chat: null }, "(10) a conversa do servidor virou diálogo");
    await page.focus("#campo-mensagem");
    let saiu = false;
    for (let i = 0; i < 25 && !saiu; i += 1) {
      await page.keyboard.press("Tab");
      saiu = await page.evaluate(() => !$("vista-conversa").contains(document.activeElement));
    }
    assert.ok(saiu, "(10) o foco ficou preso na conversa do servidor");
    await page.screenshot({ path: path.join(capturas, `v2-servidor-${navegador}-${tam}.png`) });

    // A ligação: a coluna intermediária some, a barra e o cabeçalho aparecem.
    await page.evaluate(() => { window.__foco10 = focoDaPonte; passou.length = 0; anotado.length = 0; });
    await page.evaluate(() => verSessao(window.__gabi));
    await page.waitForFunction(() => vistaPessoalAberta === window.__gabi);
    await page.waitForTimeout(800);
    for (const [quem, seletor] of Object.entries({
      "o cabeçalho": "#ligacao-cabeca", "o nome": "#sessao-ligacao", "o estado": "#ligacao-cabeca-estado",
      "a grade": "#chamada-grade", "o palco": "#palco", "o microfone": "#botao-mudo", "a surdez": "#botao-surdo",
      "AJUSTES": "#botao-server-sessao", "COMPARTILHAR": "#operador-vista", "CHAT": "#ligacao-chat",
      "DESLIGAR": "#ligacao-desligar", "a trilha": "#trilha-sessoes",
    })) assert.ok(await visivel(seletor), `(10) na ligação ${quem} (${seletor}) não está visível`);
    for (const [quem, seletor] of Object.entries({
      "criar sala de voz": "#nova-voice_room", "o perfil do operador": "#operador-quem", "a lista de salas": "#lista-voice_rooms",
      "o roster": "#lista-roster", "a conversa": "#vista-conversa",
    })) assert.ok(!(await visivel(seletor)), `(10) na ligação ${quem} (${seletor}) ainda aparece`);
    assert.match(await page.textContent("#ligacao-cabeca-estado"), /NA SALA DE VOZ/, "(10) o estado da ligação não diz que está na sala");
    // A barra fica embaixo do centro, e o cabeçalho em cima dele.
    const geometria = await page.evaluate(() => {
      const r = (s) => document.querySelector(s).getBoundingClientRect();
      return { cabeca: r("#ligacao-cabeca"), grade: r("#chamada-grade"), mudo: r("#botao-mudo"), desligar: r("#ligacao-desligar"), trilha: r(".trilha"), vw: innerWidth, vh: innerHeight };
    });
    assert.ok(geometria.cabeca.bottom <= geometria.grade.top + 1, "(10) o cabeçalho não está acima da grade");
    assert.ok(geometria.mudo.top >= geometria.grade.bottom - 1 && geometria.desligar.top >= geometria.grade.bottom - 1, "(10) a barra não está abaixo da grade");
    assert.ok(geometria.mudo.left >= geometria.trilha.right - 1, "(10) a barra invadiu o trilho das sessões");
    await page.screenshot({ path: path.join(capturas, `v2-ligacao-${navegador}-${tam}.png`) });

    // A barra chama os comandos reais.
    await page.click("#botao-mudo");
    await page.click("#botao-surdo");
    await page.waitForTimeout(300);
    const daBarra = await page.evaluate(() => passou.slice());
    assert.ok(daBarra.includes("set_muted") && daBarra.includes("set_total_isolation"), `(10) microfone ou surdez não chamaram os comandos reais: ${daBarra}`);
    await page.click("#botao-mudo");
    await page.click("#botao-surdo");

    // O chat: diálogo acessível, envio real, Escape, foco de volta.
    await page.click("#ligacao-chat");
    await page.waitForTimeout(400);
    const aberto = await page.evaluate(() => ({
      papel: $("vista-conversa").getAttribute("role"), modal: $("vista-conversa").getAttribute("aria-modal"),
      foco: document.activeElement?.id, expandido: $("ligacao-chat").getAttribute("aria-expanded"),
    }));
    assert.deepEqual(aberto, { papel: "dialog", modal: "true", foco: "campo-mensagem", expandido: "true" }, `(10) o chat não abriu como diálogo: ${JSON.stringify(aberto)}`);
    assert.ok(await visivel("#vista-conversa") && await visivel("#campo-mensagem") && await visivel("#chamada-grade"), "(10) o painel do chat, ou a grade por trás, não está visível");
    const painel = await page.evaluate(() => { const r = $("vista-conversa").getBoundingClientRect(); return { esq: r.left, dir: r.right, vw: innerWidth }; });
    assert.ok(painel.dir <= painel.vw + 1 && painel.esq >= 0, `(10) o painel do chat passa da janela: ${JSON.stringify(painel)}`);
    await page.fill("#campo-mensagem", "oi da prévia v2");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(500);
    const enviada = await page.evaluate(() => anotado.filter((a) => a.comando === "send_message").map((a) => `${a.sessao}:${a.args?.body}`));
    assert.deepEqual(enviada, ["Gabi:oi da prévia v2"], `(10) o envio do chat não passou pelo caminho real, na sessão da ligação: ${enviada}`);
    for (const tecla of ["Tab", "Shift+Tab"]) {
      for (let i = 0; i < 14; i += 1) {
        await page.keyboard.press(tecla);
        const dentro = await page.evaluate(() => $("vista-conversa").contains(document.activeElement));
        assert.ok(dentro, `(10) o foco escapou do chat com ${tecla} (passo ${i}): ${await page.evaluate(() => document.activeElement?.id)}`);
      }
    }
    await page.screenshot({ path: path.join(capturas, `v2-chat-aberto-${navegador}-${tam}.png`) });
    await page.keyboard.press("Escape");
    await page.waitForTimeout(300);
    const fechado = await page.evaluate(() => ({ papel: $("vista-conversa").getAttribute("role"), foco: document.activeElement?.id, chat: $("tela-sessao").dataset.chat ?? null }));
    assert.deepEqual(fechado, { papel: null, foco: "ligacao-chat", chat: null }, `(10) o Escape não fechou o chat devolvendo o foco: ${JSON.stringify(fechado)}`);
    assert.ok(!(await visivel("#vista-conversa")) && await visivel("#chamada-grade"), "(10) fechar o chat deixou a conversa na tela, ou tirou a grade");
    // O chat aberto, e a troca pela trilha: fecha o diálogo, solta o foco, e a volta não o reabre.
    await page.click("#ligacao-chat");
    await page.waitForTimeout(300);
    await page.evaluate(() => verSessao(1));
    await page.waitForFunction(() => $("tela-sessao").dataset.vista === "servidor");
    await page.waitForTimeout(500);
    const aposTroca = await page.evaluate(() => ({ papel: $("vista-conversa").getAttribute("role"), chat: $("tela-sessao").dataset.chat ?? null, modal: $("vista-conversa").getAttribute("aria-modal") }));
    assert.deepEqual(aposTroca, { papel: null, chat: null, modal: null }, `(10) trocar de sessão não fechou o chat: ${JSON.stringify(aposTroca)}`);
    await page.focus("#campo-mensagem");
    let soltou = false;
    for (let i = 0; i < 25 && !soltou; i += 1) {
      await page.keyboard.press("Tab");
      soltou = await page.evaluate(() => !$("vista-conversa").contains(document.activeElement));
    }
    assert.ok(soltou, "(10) o foco continuou preso depois de trocar de sessão");
    await page.evaluate(() => verSessao(window.__gabi));
    await page.waitForFunction(() => vistaPessoalAberta === window.__gabi);
    await page.waitForTimeout(600);
    assert.ok(!(await visivel("#vista-conversa")) && (await page.evaluate(() => $("tela-sessao").dataset.chat ?? null)) === null, "(10) voltar à ligação reabriu o chat sozinho");
    // O foco de voz não migrou em nada disto, e nenhum comando de voz saiu sozinho.
    const fim10 = await page.evaluate(() => ({ foco: focoDaPonte, antes: window.__foco10, passou: passou.slice() }));
    assert.equal(fim10.foco, fim10.antes, "(10) o foco de voz migrou sozinho");
    assert.ok(!fim10.passou.some((c) => ["foco_de_voz", "focar_voz", "enter_voice_room", "leave_voice_room"].includes(c)), `(10) saiu um comando de voz sozinho: ${fim10.passou}`);
    // DESLIGAR, na barra: só esta sessão.
    await page.evaluate(() => { passou.length = 0; anotado.length = 0; });
    await page.click("#ligacao-desligar");
    await page.waitForTimeout(700);
    const aposDesligar = await saidas();
    assert.ok(!aposDesligar.passou.includes("encerrar_a_ligacao"), `(10) DESLIGAR fechou a sala pessoal: ${aposDesligar.passou}`);
    assert.deepEqual(aposDesligar.anotado.filter((c) => /encerrar/.test(c)), ["Gabi:encerrar"], `(10) DESLIGAR tocou noutra sessão: ${aposDesligar.anotado}`);
    assert.ok(aposDesligar.sessoes.includes("Casa do Lago") && !aposDesligar.sessoes.includes("Gabi"), `(10) as outras sessões não continuam: ${aposDesligar.sessoes}`);

    // O celular: uma coluna, sem rolagem lateral, barra e chat dentro da janela.
    await nova("Hana", "7HHH-AAAA-BBBB-CCCC", true);
    await page.evaluate(() => { window.__hana = window.__nova; });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.evaluate(() => verSessao(window.__hana));
    await page.waitForFunction(() => vistaPessoalAberta === window.__hana);
    await page.waitForTimeout(800);
    const celular = await page.evaluate(() => ({ lateral: document.documentElement.scrollWidth > innerWidth + 1, vw: innerWidth }));
    assert.ok(!celular.lateral, "(10) a ligação rola na horizontal no celular");
    for (const [quem, seletor] of Object.entries({ "o cabeçalho": "#ligacao-cabeca", "o microfone": "#botao-mudo", "CHAT": "#ligacao-chat", "DESLIGAR": "#ligacao-desligar", "a grade": "#chamada-grade" })) {
      assert.ok(await visivel(seletor), `(10) no celular ${quem} (${seletor}) não está visível`);
    }
    const caixas = await page.evaluate(() => ["#ligacao-chat", "#ligacao-desligar", "#botao-mudo", "#operador-sair"].map((s) => { const r = document.querySelector(s).getBoundingClientRect(); return [s, r.left >= 0 && r.right <= innerWidth && r.bottom <= innerHeight]; }));
    assert.deepEqual(caixas.filter(([, ok]) => !ok), [], `(10) botões da barra fora da janela no celular: ${JSON.stringify(caixas)}`);
    await page.screenshot({ path: path.join(capturas, `v2-ligacao-celular-${navegador}.png`) });
    await page.click("#ligacao-chat");
    await page.waitForTimeout(400);
    const chatCel = await page.evaluate(() => { const r = $("vista-conversa").getBoundingClientRect(); return { esq: r.left, dir: r.right, vw: innerWidth }; });
    assert.ok(chatCel.esq >= -1 && chatCel.dir <= chatCel.vw + 1, `(10) o chat passa da janela no celular: ${JSON.stringify(chatCel)}`);
    await page.screenshot({ path: path.join(capturas, `v2-chat-celular-${navegador}.png`) });
    await page.keyboard.press("Escape");
    await page.setViewportSize(VIEWPORT);
    await page.waitForTimeout(300);
    const repetidos = await page.evaluate(() => {
      const vistos = new Map();
      for (const el of document.querySelectorAll("[id]")) vistos.set(el.id, (vistos.get(el.id) ?? 0) + 1);
      return [...vistos].filter(([, n]) => n > 1).map(([id]) => id);
    });
    assert.deepEqual(repetidos, [], `(10) ids duplicados: ${repetidos}`);

    // ------------------------------------- parte 2: a entrada (Tauri simulado)
    const entrada = await browser.newPage({ viewport: VIEWPORT });
    entrada.on("pageerror", (e) => erros.push(e.message));
    const tabela = respostas({
      meu_codigo: "K9P2-CSM7-NBFR-AYQ3",
      estado_da_sala: { estado: "NoAr", registro: { registro: "Registrado" }, firewall: null },
      amigos: [
        { codigo: "AAAA-BBBB-CCCC-DDDD", nome: "Ana Beatriz de Albuquerque Montenegro Silva", apelido: "ana", visto_em: null, direcao: "recebi", bloqueado: false, amigo: true, alvo: "@AAAA-BBBB-CCCC-DDDD" },
        { codigo: "EEEE-FFFF-GGGG-HHHH", nome: "Beto", apelido: null, visto_em: null, direcao: null, bloqueado: true, amigo: true, alvo: "@EEEE-FFFF-GGGG-HHHH" },
      ],
      contatos_da_agenda: [],
      conhecidos: [
        { alvo: "aurora.exemplo:8383", apelido: "Alex", nome: "Servidor Fictício Aurora", visto_em: Math.floor(Date.now() / 1000) - 7200, icone: null },
        { alvo: "oficina.exemplo:8383", apelido: "Alex", nome: "Oficina de Mods (exemplo)", visto_em: Math.floor(Date.now() / 1000) - 86400 * 3, icone: null },
      ],
      sessoes: null,
    });
    await entrada.addInitScript((t) => {
      window.__cmds = [];
      window.__TAURI__ = {
        core: { invoke: async (cmd) => { window.__cmds.push(cmd); return Object.hasOwn(t, cmd) ? t[cmd] : null; } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested: () => {}, isMaximized: async () => false }) },
      };
    }, tabela);
    await entrada.goto(url);
    await entrada.waitForTimeout(900);
    await entrada.evaluate(MEDIDOR);
    for (const [quem, seletor] of Object.entries({
      "o bloco SEU CÓDIGO": "#boot-codigo",
      "o código": "#boot-codigo-valor",
      "COPIAR": "#boot-copiar-codigo",
      "a frase da sala": "#boot-codigo-estado",
      "a lista de AMIGOS": "#boot-amigos",
      "o LIGAR do amigo": "#boot-amigos-lista button",
      "o campo de adicionar": "#boot-amigo-codigo",
    })) {
      assert.ok(await entrada.evaluate((s) => window.__visivel(s), seletor), `na entrada ${quem} (${seletor}) não está visível`);
    }
    const ligares = await entrada.evaluate(() => [...document.querySelectorAll("#boot-amigos-lista button")].map((b) => [b.textContent, b.disabled]));
    assert.ok(ligares.some(([t, d]) => t === "LIGAR" && d === true), `o amigo bloqueado pode ser ligado: ${JSON.stringify(ligares)}`);
    assert.ok(ligares.some(([t, d]) => t === "LIGAR" && d === false), `o amigo livre não pode ser ligado: ${JSON.stringify(ligares)}`);
    assert.match(await entrada.evaluate(() => document.getElementById("boot-codigo-estado").textContent), /(?:pode|consegue) ligar/, "a sala registrada não diz que recebe");
    const medidas = await entrada.evaluate(() => {
      const caixa = document.querySelector(".boot-caixa").getBoundingClientRect();
      const cartoes = [...document.querySelectorAll(".amigo-cartao")].map((c) => {
        const cod = c.querySelector(".amigo-codigo");
        const altura = parseFloat(getComputedStyle(cod).lineHeight) || cod.getBoundingClientRect().height;
        const linhasDoCodigo = Math.round(cod.getBoundingClientRect().height / altura);
        const acoes = [...c.querySelectorAll(".amigo-acoes button")].map((b) => b.getBoundingClientRect());
        return {
          linhasDoCodigo,
          acaoFora: acoes.some((r) => r.right > caixa.right + 0.5 || r.left < caixa.left - 0.5),
          // A lista rola, mas a primeira pessoa tem de estar inteira à vista.
          cortada: c === document.querySelector(".amigo-cartao") && acoes.some((r) => r.bottom > c.parentElement.getBoundingClientRect().bottom + 0.5),
          cartaoAlto: c.getBoundingClientRect().height,
          codigoLargura: cod.getBoundingClientRect().width,
        };
      });
      return { cartoes, rolagemX: document.documentElement.scrollWidth > innerWidth + 1, caixaRola: document.querySelector(".boot-caixa").scrollWidth > document.querySelector(".boot-caixa").clientWidth + 1 };
    });
    assert.equal(medidas.cartoes.length, 2, "esperava dois amigos na entrada");
    for (const c of medidas.cartoes) {
      assert.equal(c.linhasDoCodigo, 1, `o código quebrou em ${c.linhasDoCodigo} linhas: ${JSON.stringify(c)}`);
      assert.ok(c.codigoLargura > 120, `o código está espremido: ${c.codigoLargura}px`);
      assert.ok(!c.cortada, `as ações do primeiro amigo estão cortadas pela lista: ${JSON.stringify(c)}`);
      assert.ok(!c.acaoFora,`uma ação do amigo saiu da caixa: ${JSON.stringify(c)}`);
      assert.ok(c.cartaoAlto < 200, `o cartão do amigo ficou enorme: ${c.cartaoAlto}px`);
    }
    assert.ok(!medidas.rolagemX && !medidas.caixaRola, `a entrada rola na horizontal: ${JSON.stringify(medidas)}`);
    await entrada.screenshot({ path: path.join(capturas, `entrada-codigo-amigos-${navegador}-${tam}.png`) });

    // A entrada v2 em largura total, em quatro janelas: as colunas lado a lado
    // onde cabem, nada sobreposto, nada fora da janela, e cada botão visível.
    const visivelE = (s) => entrada.evaluate((x) => window.__visivel(x), s);
    // Na janela estreita a página rola: leva o elemento à vista antes de medir.
    const visivelNaPagina = (s) => entrada.evaluate((x) => { document.querySelector(x)?.scrollIntoView({ block: "center" }); return window.__visivel(x); }, s);
    const colunas = () => entrada.evaluate(() => [".inicio-conectar", "#boot-codigo", "#boot-amigos", "#boot-servidores", ".inicio-hospedar"].map((c) => {
      const r = document.querySelector(c).getBoundingClientRect();
      return { c, esq: r.left, dir: r.right, topo: r.top, base: r.bottom };
    }));
    const sobrepoe = (a, b) => a.esq < b.dir - 1 && b.esq < a.dir - 1 && a.topo < b.base - 1 && b.topo < a.base - 1;
    for (const [rotulo, largura, altura, lado] of [["1920x1080", 1920, 1080, 2], ["1280x800", 1280, 800, 2], ["1000x760", 1000, 760, 2], ["celular-390x844", 390, 844, 1]]) {
      await entrada.setViewportSize({ width: largura, height: altura });
      await entrada.waitForTimeout(450);
      const cs = await colunas();
      for (let i = 0; i < cs.length; i += 1) for (let j = i + 1; j < cs.length; j += 1) {
        assert.ok(!sobrepoe(cs[i], cs[j]), `(v2 ${rotulo}) as colunas ${cs[i].c} e ${cs[j].c} se sobrepõem: ${JSON.stringify(cs)}`);
      }
      const lateral = cs.filter((c) => Math.abs(c.topo - cs[0].topo) < 4).length;
      assert.equal(lateral, lado, `(v2 ${rotulo}) esperava ${lado} coluna(s) lado a lado e achou ${lateral}: ${JSON.stringify(cs)}`);
      assert.ok(!(await entrada.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1)), `(v2 ${rotulo}) a entrada rola na horizontal`);
      for (const [quem, seletor] of Object.entries({
        "a lista de SERVIDORES": "#boot-servidores-lista button", "CONECTAR": "#botao-conectar", "o código": "#boot-codigo-valor",
        "COPIAR": "#boot-copiar-codigo", "o LIGAR": "#boot-amigos-lista button", "ADICIONAR": "#boot-amigo-adicionar",
        "HOSPEDAR AQUI": "#botao-hospedar", "o PERFIL": "#boot-perfil", "a engrenagem": "#botao-server",
      })) assert.ok(await visivelNaPagina(seletor), `(v2 ${rotulo}) ${quem} (${seletor}) não está visível`);
      const fora = await entrada.evaluate(() => ["#botao-conectar", "#botao-hospedar", "#boot-perfil", "#boot-copiar-codigo"].map((s) => { const r = document.querySelector(s).getBoundingClientRect(); return [s, r.left >= 0 && r.right <= innerWidth]; }).filter(([, ok]) => !ok));
      assert.deepEqual(fora, [], `(v2 ${rotulo}) botões fora da janela: ${JSON.stringify(fora)}`);
      await entrada.screenshot({ path: path.join(capturas, `v2-entrada-${navegador}-${rotulo}.png`) });
    }
    await entrada.setViewportSize(VIEWPORT);
    await entrada.waitForTimeout(300);

    // O modal do PERFIL e a confirmação: o mesmo respiro em todas as seções, e
    // FECHAR e as ações ao alcance também em 390 px.
    for (const [rotulo, largura, altura] of [["1280", 1280, 800], ["390", 390, 844]]) {
      await entrada.setViewportSize({ width: largura, height: altura });
      await entrada.waitForTimeout(300);
      await entrada.click("#boot-perfil");
      await entrada.waitForTimeout(400);
      const modal = await entrada.evaluate(() => {
        const dentro = (r) => r.left >= 0 && r.right <= innerWidth && r.top >= 0 && r.bottom <= innerHeight;
        const caixa = document.querySelector("#perfil .nomear-caixa");
        const antes = dentro($("perfil-fechar").getBoundingClientRect());
        caixa.scrollTop = caixa.scrollHeight;
        return {
          caixaDentro: dentro(caixa.getBoundingClientRect()), fecharDentro: antes, fecharDepoisDeRolar: dentro($("perfil-fechar").getBoundingClientRect()),
          paddingDoForm: getComputedStyle($("perfil-forma")).paddingLeft, paddingDaSala: getComputedStyle(document.querySelector(".perfil-cartao-codigo")).paddingLeft,
          paddingDaCabeca: getComputedStyle(document.querySelector("#perfil .nomear-cabeca")).paddingLeft,
        };
      });
      assert.ok(modal.caixaDentro, `(v2 perfil ${rotulo}) a caixa do perfil passa da janela: ${JSON.stringify(modal)}`);
      assert.ok(modal.fecharDentro && modal.fecharDepoisDeRolar, `(v2 perfil ${rotulo}) o FECHAR some ao rolar: ${JSON.stringify(modal)}`);
      assert.equal(modal.paddingDaSala, modal.paddingDoForm, `(v2 perfil ${rotulo}) a seção SEU CÓDIGO tem outro respiro que o formulário: ${JSON.stringify(modal)}`);
      assert.ok(parseFloat(modal.paddingDaCabeca) >= 16, `(v2 perfil ${rotulo}) a cabeça do modal tem outro respiro: ${JSON.stringify(modal)}`);
      await entrada.screenshot({ path: path.join(capturas, `v2-perfil-${navegador}-${rotulo}.png`) });
      await entrada.keyboard.press("Escape");
      await entrada.waitForTimeout(250);
      await entrada.evaluate(() => abrirConfirmacao("SAIR DE UM SERVIDOR COM NOME BEM COMPRIDO", "Sair daqui encerra a conversa com o servidor e você precisa entrar de novo. ".repeat(6), "SAIR", () => {}));
      await entrada.waitForTimeout(300);
      const conf = await entrada.evaluate(() => ["#moderar-confirmar", "#moderar-cancelar"].map((s) => {
        const r = document.querySelector(s).getBoundingClientRect();
        return [s, window.__visivel(s) && r.left >= 0 && r.right <= innerWidth && r.top >= 0 && r.bottom <= innerHeight];
      }));
      assert.deepEqual(conf.filter(([, ok]) => !ok), [], `(v2 confirmação ${rotulo}) ações fora da janela: ${JSON.stringify(conf)}`);
      await entrada.screenshot({ path: path.join(capturas, `v2-confirmacao-${navegador}-${rotulo}.png`) });
      await entrada.click("#moderar-cancelar");
      await entrada.waitForTimeout(250);
    }
    await entrada.setViewportSize(VIEWPORT);
    await entrada.waitForTimeout(300);

    // Nenhuma presença: só informação real (sem «online», «ativo», «disponível»).
    const presenca = await entrada.evaluate(() => /\bonline\b|\bativo\b|disponível|visto agora/i.test(document.getElementById("tela-boot").innerText));
    assert.ok(!presenca, "(v2) a entrada escreve presença que o produto não tem");

    // Os botões chamam os handlers reais.
    await entrada.evaluate(() => { window.__cmds.length = 0; });
    await entrada.click("#botao-conectar");
    await entrada.waitForTimeout(250);
    assert.ok(await visivelE("#servidores-endereco"), "(v2) CONECTAR não abriu o diálogo real");
    await entrada.keyboard.press("Escape");
    await entrada.waitForTimeout(200);
    await entrada.click("#boot-perfil");
    await entrada.waitForTimeout(250);
    assert.ok(await visivelE("#perfil"), "(v2) o PERFIL não abriu o diálogo real");
    await entrada.keyboard.press("Escape");
    await entrada.waitForTimeout(200);
    await entrada.evaluate(() => { window.__cmds.length = 0; });
    await entrada.click("#boot-servidores-lista button.servidor-linha");
    await entrada.waitForTimeout(500);
    const daLinha = await entrada.evaluate(() => window.__cmds.slice());
    assert.ok(daLinha.includes("conferir_esta_versao") || daLinha.includes("connect"), `(v2) a linha do servidor não chegou ao connect real: ${daLinha}`);
    await entrada.evaluate(() => { for (const t of document.querySelectorAll("section.tela")) t.hidden = true; document.getElementById("tela-boot").hidden = false; });
    await entrada.evaluate(() => { window.__cmds.length = 0; });
    await entrada.click("#botao-hospedar");
    await entrada.waitForTimeout(500);
    const dohospedar = await entrada.evaluate(() => window.__cmds.slice());
    assert.ok(dohospedar.includes("meu_servidor"), `(v2) HOSPEDAR AQUI não chamou o handler real: ${dohospedar}`);
    await entrada.evaluate(() => { for (const t of document.querySelectorAll("section.tela")) t.hidden = true; document.getElementById("tela-boot").hidden = false; });
    await entrada.waitForTimeout(300);

    // O movimento: a entrada chega com uma animação curta, uma vez; sem ela sob
    // `prefers-reduced-motion` e com a voz aberta.
    const nome = () => entrada.evaluate(() => { const e = getComputedStyle(document.querySelector(".boot-centro")); return [e.animationName, e.animationIterationCount, e.animationDuration]; });
    const normal = await nome();
    assert.deepEqual(normal, ["entrada-chega", "1", "0.24s"], `(v2) a animação de entrada não é curta e única: ${normal}`);
    await entrada.emulateMedia({ reducedMotion: "reduce" });
    assert.equal((await nome())[0], "none", "(v2) a animação continua com prefers-reduced-motion");
    await entrada.emulateMedia({ reducedMotion: "no-preference" });
    await entrada.evaluate(() => { document.documentElement.dataset.vozAberta = "sim"; });
    assert.equal((await nome())[0], "none", "(v2) a animação continua com a voz aberta");
    await entrada.evaluate(() => { document.documentElement.dataset.vozAberta = "nao"; });
    const repetidosE = await entrada.evaluate(() => {
      const vistos = new Map();
      for (const el of document.querySelectorAll("[id]")) vistos.set(el.id, (vistos.get(el.id) ?? 0) + 1);
      return [...vistos].filter(([, n]) => n > 1).map(([id]) => id);
    });
    assert.deepEqual(repetidosE, [], `(v2) ids duplicados na entrada: ${repetidosE}`);

    assert.deepEqual(erros, [], `erros de página: ${erros}`);
    console.log(`ok: vista pessoal e entrada medidas em ${navegador} (${VIEWPORT.width}×${VIEWPORT.height}); capturas em ${capturas}`);
  } finally {
    await browser.close();
    server.close();
  }
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
