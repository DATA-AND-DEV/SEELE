// **Quem usa pela primeira vez fala sem tecla — e a tela conta isso a ele.**
//
// Pedido do dono (13/11/2026): «o padrão agora para novos usuários, quando eles
// forem usar pela primeira vez, é o microfone aberto, não usando o botão». O
// Rust decide quem é novo (`Preferences::assentar_o_padrao_do_microfone`, e os
// testes de lá). Esta bancada confere a outra metade: o que a janela mostra a
// quem chegou com o microfone abrindo por voz, e a quem continuou na tecla.
//
//   1. **Sem sessão, um dos três modos está marcado**: o que vai valer. Antes,
//      quem nunca escolheu via três botões apagados.
//   2. **Ao entrar numa sala com o microfone por voz, uma faixa diz** que ele
//      abre sozinho, onde se vê que está sendo ouvido e onde se troca para a
//      tecla — com os nomes que a configuração tem.
//   3. **Falando, o nome da pessoa na sala diz «fala»**, que é onde ela vê que
//      está no ar.
//   4. **ENTENDI guarda que foi lida**: a faixa não volta, nem depois de
//      recarregar. Escolher um modo em CONFIGURAÇÕES também guarda; um modo que
//      o disco recusou gravar, não.
//   5. **Quem ficou na tecla não vê faixa nenhuma**, e a tela dele continua
//      dizendo a tecla.
//   6. **Com o microfone que não abriu, a faixa se cala**: «abre quando você
//      fala» de um microfone fechado seria mentira.
//
// # O que é simulado, e o que é o produto
//
// SIMULADO: a ponte (`window.__TAURI__` com respostas combinadas) e o retrato.
// REAL: o `index.html`, todo o CSS e todo o JS de `ui/`.
//
// Rodar:  node apps/seele-app/bancada/microfone-por-voz.cjs

const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

/** O retrato de quem está na sala, com o modo dito. */
function naSala(modo, extra = {}) {
  return retrato({ voice_mode: modo, ...extra });
}

/** O mesmo, com a pessoa falando: o retrato e a linha dela na sala. */
function falando(modo) {
  const r = naSala(modo, { speaking: true });
  r.voice_rooms[0].people[0].speaking = true;
  return r;
}

/** Fora de sala: a sala existe, e a pessoa não está nela. */
function foraDeSala(modo) {
  const r = naSala(modo);
  r.voice_rooms[0].occupied_by_us = false;
  r.voice_rooms[0].people = r.voice_rooms[0].people.filter((p) => !p.is_self);
  return r;
}

/** Uma aba com a ponte de mentira. `gravado` é o que `modo_de_voz_escolhido` responde. */
async function abrir(contexto, url, gravado) {
  const aba = await contexto.newPage();
  const erros = [];
  aba.on("pageerror", (e) => erros.push(e.message));
  await aba.addInitScript((tabela) => {
    window.__chamadas = [];
    window.__TAURI__ = {
      core: {
        invoke: async (comando, argumentos) => {
          window.__chamadas.push([comando, argumentos ?? null]);
          // Sem sessão, o `snapshot` do produto recusa com este nome.
          if (comando === "snapshot") {
            if (window.__retrato) return window.__retrato;
            throw "NotConnected";
          }
          if (comando === "set_voice_mode") {
            if (window.__discoRecusa) throw "NaoGravei";
            window.__gravado = argumentos.mode;
            if (window.__retrato) window.__retrato.voice_mode = argumentos.mode;
            return null;
          }
          if (comando === "modo_de_voz_escolhido") return window.__gravado;
          return Object.hasOwn(tabela, comando) ? tabela[comando] : null;
        },
      },
      event: { listen: async () => () => {} },
      window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
    };
  }, respostas({ microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
  await aba.addInitScript((modo) => { window.__gravado = modo; }, gravado);
  await aba.goto(url);
  await aba.waitForTimeout(500);
  return { aba, erros };
}

/** Desenha a sessão com este retrato, como o tique do produto faz. */
async function desenhar(aba, r) {
  await aba.evaluate((quadro) => {
    window.__retrato = quadro;
    for (const secao of document.querySelectorAll("section.tela")) secao.hidden = true;
    document.getElementById("tela-sessao").hidden = false;
    window.desenhar(quadro);
  }, r);
  await aba.waitForTimeout(120);
}

/** O que a pessoa lê sobre o microfone, neste instante. */
function ler(aba) {
  return aba.evaluate(() => {
    const faixa = document.getElementById("aviso-microfone");
    const caixa = faixa.getBoundingClientRect();
    const botao = document.getElementById("aviso-microfone-fechar").getBoundingClientRect();
    const eu = [...document.querySelectorAll("#lista-voice_rooms .voice_room-pessoa")]
      .find((linha) => linha.querySelector(".voice_room-pessoa-eu"));
    return {
      faixaAVista: faixa.checkVisibility(),
      faixaDiz: document.getElementById("aviso-microfone-texto").textContent,
      faixaCabe: caixa.left >= -0.5 && caixa.right <= innerWidth + 0.5
        && botao.left >= caixa.left - 0.5 && botao.right <= caixa.right + 0.5
        && document.documentElement.scrollWidth <= innerWidth + 1,
      linha: document.getElementById("voz-estado").textContent,
      botao: document.getElementById("botao-mudo").getAttribute("aria-label"),
      minhaMarca: eu?.querySelector(".voice_room-pessoa-marca")?.textContent ?? null,
      lido: localStorage.getItem("seele.aviso-do-microfone-por-voz.v1"),
    };
  });
}

/** Os modos da configuração: qual está marcado, e o nome de cada um. */
async function modosNaConfiguracao(aba, origem) {
  await aba.evaluate((de) => abrirServer(de), origem);
  await aba.waitForTimeout(200);
  return aba.evaluate(() => ({
    marcados: [...document.querySelectorAll('.server-modo[aria-pressed="true"]')].map((b) => b.dataset.modo),
    nomes: Object.fromEntries([...document.querySelectorAll(".server-modo")]
      .map((b) => [b.dataset.modo, b.querySelector(".server-modo-nome").textContent.trim()])),
    secao: document.getElementById("secao-audio").dataset.titulo,
    contexto: document.getElementById("server-contexto").textContent,
    atalho: document.querySelector(".server-atalho-desc").textContent.replace(/\s+/g, " ").trim(),
  }));
}

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const url = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium().launch({ headless: true });
  try {
    for (const [largura, altura] of [[1280, 860], [760, 560]]) {
      const onde = `${largura}×${altura}`;

      // ---------------------------------------------------- quem chegou
      // Os dois modos sem tecla: o sempre aberto, que é o de quem instala agora
      // (decisão do dono, 13/11/2026), e o por voz.
      for (const modo of ["Open", "VoiceActivated"]) {
      const onde = `${largura}×${altura}, ${modo}`;
      const daLinha = modo === "Open" ? "microfone aberto" : "microfone abre por voz";
      const contexto = await browser.newContext({ viewport: { width: largura, height: altura } });
      const { aba, erros } = await abrir(contexto, url, modo);
      const frase = await aba.evaluate((m) => (m === "Open" ? AVISO_DO_MICROFONE_ABERTO : AVISO_DO_MICROFONE_POR_VOZ), modo);

      // 1. Sem sessão, a configuração marca o modo que vai valer.
      const naEntrada = await modosNaConfiguracao(aba, "tela-boot");
      assert.deepEqual(naEntrada.marcados, [modo],
        `${onde}: sem sessão, a configuração não marca o modo que vai valer: ${JSON.stringify(naEntrada.marcados)}`);
      // E a linha da tecla diz que ela depende do modo, com o nome do botão.
      assert.ok(naEntrada.atalho.includes(naEntrada.nomes.PushToTalk),
        `${onde}: a linha «Falar enquanto segura» promete a tecla sem dizer o modo: «${naEntrada.atalho}»`);
      await aba.evaluate(() => fecharServer());

      // 2. Fora de sala, nada; na sala, a faixa.
      await desenhar(aba, foraDeSala(modo));
      assert.equal((await ler(aba)).faixaAVista, false, `${onde}: a faixa acendeu fora de sala`);
      await desenhar(aba, naSala(modo));
      let lido = await ler(aba);
      assert.ok(lido.faixaAVista, `${onde}: quem entrou na sala com o microfone por voz não foi avisado`);
      assert.equal(lido.faixaDiz, frase, `${onde}: a faixa não diz a frase do frases.js`);
      // Os nomes da frase são os da configuração: é para lá que ela manda.
      for (const nome of ["CONFIGURAÇÕES", naEntrada.secao, naEntrada.nomes.PushToTalk, ...(modo === "Open" ? [naEntrada.nomes.VoiceActivated, "SEMPRE ABERTO"] : [])]) {
        assert.ok(lido.faixaDiz.includes(nome), `${onde}: a faixa não diz «${nome}»: «${lido.faixaDiz}»`);
      }
      assert.ok(lido.faixaCabe, `${onde}: a faixa (ou o ENTENDI) sai da janela`);
      assert.ok(lido.linha.startsWith(daLinha), `${onde}: a linha do operador diz «${lido.linha}»`);
      assert.equal(lido.botao, daLinha.toUpperCase(), `${onde}: o botão do microfone diz «${lido.botao}»`);
      assert.equal(lido.minhaMarca, null, `${onde}: calada, a pessoa aparece como «${lido.minhaMarca}»`);
      if (process.env.SEELE_CAPTURAS) {
        await aba.screenshot({ path: require("node:path").join(process.env.SEELE_CAPTURAS, `microfone-${modo === "Open" ? "aberto" : "por-voz"}-${largura}x${altura}.png`) });
      }

      // 3. Falando, ela se vê no ar: o nome dela na sala, a linha e o botão.
      await desenhar(aba, falando(modo));
      lido = await ler(aba);
      assert.equal(lido.minhaMarca, "fala", `${onde}: falando, o nome da pessoa na sala não diz «fala»`);
      // A frase do por voz aponta a marca; a do sempre aberto aponta o botão que cala.
      if (modo === "VoiceActivated") assert.ok(lido.faixaDiz.includes(`«${lido.minhaMarca}»`), `${onde}: a faixa promete uma marca que a sala não mostra`);
      assert.ok(lido.linha.endsWith("no ar"), `${onde}: falando, a linha do operador diz «${lido.linha}»`);
      assert.equal(lido.botao, `${daLinha.toUpperCase()} · NO AR`, `${onde}: falando, o botão diz «${lido.botao}»`);

      // Sair da sala apaga; voltar sem ter lido acende de novo.
      await desenhar(aba, foraDeSala(modo));
      assert.equal((await ler(aba)).faixaAVista, false, `${onde}: a faixa ficou acesa fora de sala`);
      await desenhar(aba, naSala(modo));
      assert.ok((await ler(aba)).faixaAVista, `${onde}: a faixa não voltou para quem ainda não a leu`);

      // 6. Com o microfone que não abriu, ela se cala.
      await desenhar(aba, naSala(modo, { audio_available: false }));
      assert.equal((await ler(aba)).faixaAVista, false, `${onde}: a faixa diz que o microfone está aberto de um microfone fechado`);
      await desenhar(aba, naSala(modo));

      // 4. ENTENDI guarda, e ela não volta: nem no quadro seguinte, nem recarregando.
      await aba.click("#aviso-microfone-fechar");
      lido = await ler(aba);
      assert.equal(lido.faixaAVista, false, `${onde}: o ENTENDI não apagou a faixa`);
      assert.equal(lido.lido, "sim", `${onde}: o ENTENDI não guardou que a faixa foi lida`);
      await desenhar(aba, naSala(modo));
      assert.equal((await ler(aba)).faixaAVista, false, `${onde}: a faixa voltou no quadro seguinte ao ENTENDI`);
      await aba.reload();
      await aba.waitForTimeout(500);
      await desenhar(aba, naSala(modo));
      assert.equal((await ler(aba)).faixaAVista, false, `${onde}: a faixa voltou depois de recarregar`);
      assert.deepEqual(erros, [], `${onde}: erros de página: ${erros.join(" | ")}`);
      await contexto.close();
      }

      // ------------------------- quem chegou, e escolheu em CONFIGURAÇÕES
      const outro = await browser.newContext({ viewport: { width: largura, height: altura } });
      const { aba: aba2, erros: erros2 } = await abrir(outro, url, "VoiceActivated");
      await desenhar(aba2, naSala("VoiceActivated"));
      assert.ok((await ler(aba2)).faixaAVista, `${onde}: instalação nova, e a faixa não acendeu`);
      // O disco recusou: nada foi escolhido, e a faixa continua valendo.
      await aba2.evaluate(async () => { window.__discoRecusa = true; await escolherModo("PushToTalk"); window.__discoRecusa = false; });
      lido = await ler(aba2);
      assert.ok(lido.faixaAVista && lido.lido === null, `${onde}: um modo que o disco recusou apagou a faixa`);
      // Escolheu a tecla: achou o lugar, e a faixa não tem mais o que ensinar.
      await aba2.evaluate(() => escolherModo("PushToTalk"));
      const escolhas = await aba2.evaluate(() => window.__chamadas.filter(([c]) => c === "set_voice_mode").map(([, a]) => a));
      assert.deepEqual(escolhas.at(-1), { mode: "PushToTalk" }, `${onde}: SEGURAR TECLA mandou ${JSON.stringify(escolhas)}`);
      await desenhar(aba2, naSala("PushToTalk"));
      lido = await ler(aba2);
      assert.equal(lido.faixaAVista, false, `${onde}: a faixa ficou acesa depois de a pessoa escolher a tecla`);
      assert.equal(lido.lido, "sim", `${onde}: escolher o modo não guardou que a faixa foi lida`);
      // E voltando para a voz por escolha, ela não é dita de novo.
      await aba2.evaluate(() => escolherModo("VoiceActivated"));
      await desenhar(aba2, naSala("VoiceActivated"));
      assert.equal((await ler(aba2)).faixaAVista, false, `${onde}: quem escolheu a voz em CONFIGURAÇÕES foi avisado do que escolheu`);
      assert.deepEqual(erros2, [], `${onde}: erros de página: ${erros2.join(" | ")}`);
      await outro.close();

      // ----------------------------------------------- quem ficou na tecla
      const antigo = await browser.newContext({ viewport: { width: largura, height: altura } });
      const { aba: aba3, erros: erros3 } = await abrir(antigo, url, "PushToTalk");
      const daTecla = await modosNaConfiguracao(aba3, "tela-boot");
      assert.deepEqual(daTecla.marcados, ["PushToTalk"],
        `${onde}: sem sessão, quem já usava não vê SEGURAR TECLA marcado: ${JSON.stringify(daTecla.marcados)}`);
      await aba3.evaluate(() => fecharServer());
      await desenhar(aba3, falando("PushToTalk"));
      lido = await ler(aba3);
      assert.equal(lido.faixaAVista, false, `${onde}: quem está na tecla foi avisado de um microfone por voz`);
      assert.equal(lido.lido, null, `${onde}: a marca de lida foi gravada sem a faixa ter aparecido`);
      assert.ok(lido.linha.startsWith("microfone abre enquanto segura ESPAÇO"), `${onde}: na tecla, a linha diz «${lido.linha}»`);
      assert.deepEqual(erros3, [], `${onde}: erros de página: ${erros3.join(" | ")}`);
      await antigo.close();
    }

    // A ajuda diz os dois jeitos e onde se escolhe, sem afirmar qual vale.
    const ajuda = await browser.newContext();
    const { aba: aba4 } = await abrir(ajuda, url, "VoiceActivated");
    const diz = await aba4.evaluate(() => {
      const termo = [...document.querySelectorAll("#ajuda .ajuda-termo")].find((t) => t.textContent.trim() === "MICROFONE");
      return termo?.nextElementSibling?.textContent ?? null;
    });
    assert.ok(diz && diz.includes("CONFIGURAÇÕES · MICROFONE E SOM") && diz.includes("quando você fala") && diz.includes("tecla"),
      `a ajuda não diz como o microfone abre nem onde se escolhe: «${diz}»`);
    await ajuda.close();

    console.log("ok: quem chega fala de microfone aberto (ou por voz) e é avisado uma vez (onde se vê no ar, onde se troca); quem ficou na tecla não vê nada; sem sessão a configuração marca o modo que vale");
  } finally {
    await browser.close();
    server.close();
  }
})().catch((falha) => { console.error(falha); process.exit(1); });
