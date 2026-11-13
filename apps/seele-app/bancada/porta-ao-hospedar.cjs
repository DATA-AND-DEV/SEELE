// **Quem pode entrar, escolhido na hora de hospedar** (pedido do dono,
// 13/11/2026: «quando você hospeda um servidor, já tem que mostrar se ele quer
// senha ou não na hora de configurar»). Com o HTML e os scripts reais; a ponte
// do Tauri é um duble. Não há Rust do outro lado: que o servidor nasce mesmo
// com a senha, antes da primeira conexão, é de `a_porta_de_um_servidor_novo`
// (`cargo test -p seele-app`).
//
// # O que ela mede, em 720×480, 900×600 e 1600×919
//
// 1. **O passo pergunta.** Na tela de preparar de um servidor NOVO há um grupo
//    de opções de verdade (um `radiogroup` com nome, dois rádios nativos do
//    mesmo grupo, cada um ligado à linha que o explica), e COM A SUA APROVAÇÃO
//    vem marcado. Os campos da senha não existem até COM SENHA ser escolhido.
// 2. **Nada fora da janela.** A página e o conteúdo não rolam de lado, a barra
//    de ações inteira está na janela, e cada controle do cartão, rolado à
//    vista, cabe inteiro acima dela.
// 3. **O teclado.** As setas trocam a escolha; o Tab leva do grupo ao campo da
//    senha, que tem rótulo; Enter num campo é o HOSPEDAR; Esc é o VOLTAR.
// 4. **Com aprovação**, o `hospedar` vai com `senha: null`.
// 5. **A senha que não serve é recusada logo abaixo do campo**, com o foco nele
//    e a frase à vista sem rolar, e nada é criado nem sobe: vazia, só espaços,
//    com espaço na ponta, curta (com o mínimo que a ponte mandou, e não um
//    número daqui), longa, e as duas diferentes.
// 6. **MOSTRAR** troca os dois campos para texto e vira OCULTAR.
// 7. **Com senha**, o `hospedar` vai com a senha, **depois** do
//    `criar_servidor`, e os campos ficam vazios.
// 8. **A senha não vai a mais nenhum lugar**: nem ao console, nem a outro
//    comando da ponte, nem a `data-*`, nem ao HTML, nem ao armazenamento.
// 9. **Se o servidor não subir**, a frase diz que a senha não foi gravada.
// 10. **Quem adota o servidor que já estava aqui não ganha a pergunta.**
//
// Rodar:  node apps/seele-app/bancada/porta-ao-hospedar.cjs [pasta-das-capturas]

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { servir, respostas } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");

const JANELAS = [
  { width: 720, height: 480 },
  { width: 900, height: 600 },
  { width: 1600, height: 919 },
];

const ANFITRIAO = {
  aqui: "127.0.0.1:8383",
  convite: "seele://casa.exemplo:8383#bancada",
  alcance: "PortaNoRoteador",
  porta_recusada: null,
  encontro_recusado: null,
  firewall: null,
  porta_nao_canonica: null,
};

const LINHAS = [
  { linha: "2", servidor: "2.0", apis: [7, 6], app: null, esta: true, instalada: true, tem_pacote: true, perde_conversas: false },
  { linha: "1", servidor: "1.0", apis: [6], app: "0.16.0", esta: false, instalada: true, tem_pacote: true, perde_conversas: false },
];

/** A senha da bancada. Um texto que não aparece em nenhuma frase da tela. */
const SENHA = "zeruel-quatorze";

const pasta = process.argv[2] && !process.argv[2].startsWith("--") ? process.argv[2] : null;
if (pasta) fs.mkdirSync(pasta, { recursive: true });

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium().launch({ headless: true });
  try {
    /** Abre a casca com o duble, e devolve a página e o que ela disse. */
    const abrir = async (viewport, extra = {}, falhas = {}) => {
      const page = await browser.newPage({ viewport });
      const erros = [];
      const console_ = [];
      page.on("pageerror", (e) => erros.push(e.message));
      page.on("console", (m) => console_.push(m.text()));
      await page.addInitScript(({ tabela, falhas }) => {
        window.chamadas = [];
        window.__TAURI__ = {
          core: { invoke: async (cmd, args) => {
            chamadas.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
            if (cmd in falhas) throw falhas[cmd];
            // `connect` fica pendente: o que se mede é o que foi pedido antes.
            if (cmd === "connect") return new Promise(() => {});
            return tabela[cmd] ?? null;
          } },
          event: { listen: async () => () => {} },
          window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
        };
      }, { tabela: respostas({
        linhas_para_hospedar: LINHAS,
        regras_do_icone_do_server: { limite_bytes: 8192, lado: 256 },
        regras_da_senha_do_server: { minimo: 8, maximo_em_bytes: 128 },
        criar_servidor: { id: "srv-novo", nome: "", versao: "1.1.1", caminho: "servidores/srv-novo" },
        hospedar: ANFITRIAO,
        ...extra,
      }), falhas });
      await page.goto(`http://127.0.0.1:${server.address().port}`);
      await page.waitForTimeout(300);
      return { page, erros, console_ };
    };

    /** HOSPEDAR AQUI, sem servidor guardado: a tela de preparar de um novo. */
    const preparar = async (page) => {
      await page.click("#botao-hospedar");
      await page.waitForFunction(() => !$("tela-preparar").hidden && $("preparar-porta-portao-nota").textContent !== "");
      await page.waitForTimeout(150);
    };

    const pedidos = (page, nomes) => page.evaluate((n) => chamadas.filter((c) => n.includes(c.cmd)), nomes);
    const esquecer = (page) => page.evaluate(() => { chamadas.length = 0; });

    /** O que está fora da janela, ou fora de alcance, na tela de preparar. */
    const fora = (page, controles) => page.evaluate((controles) => {
      const raiz = $("tela-preparar");
      const rolagem = raiz.querySelector(".area-conteudo");
      const barra = raiz.querySelector(".area-barra").getBoundingClientRect();
      const problemas = [];
      const doc = document.documentElement;
      if (doc.scrollWidth > window.innerWidth) problemas.push(`a página rola de lado (${doc.scrollWidth} > ${window.innerWidth})`);
      if (rolagem.scrollWidth > rolagem.clientWidth + 1) problemas.push(`o conteúdo rola de lado (${rolagem.scrollWidth} > ${rolagem.clientWidth})`);
      if (barra.top < 0 || barra.bottom > window.innerHeight + 0.5 || barra.left < 0 || barra.right > window.innerWidth + 0.5) {
        problemas.push(`a barra de ações sai da janela: ${JSON.stringify(barra)}`);
      }
      for (const botao of raiz.querySelectorAll(".area-barra button")) {
        const c = botao.getBoundingClientRect();
        if (c.top < 0 || c.bottom > window.innerHeight + 0.5 || c.left < 0 || c.right > window.innerWidth + 0.5) {
          problemas.push(`${botao.id} sai da janela`);
        }
      }
      for (const el of raiz.querySelectorAll(".preparar-porta *")) {
        const c = el.getBoundingClientRect();
        if (c.width === 0 || c.height === 0) continue;
        if (c.left < -0.5 || c.right > window.innerWidth + 0.5) problemas.push(`${el.id || el.className} passa da borda lateral`);
        if (el.scrollWidth > el.clientWidth + 1 && getComputedStyle(el).overflowX !== "visible") {
          problemas.push(`${el.id || el.className} corta o próprio texto`);
        }
      }
      // Cada controle, rolado à vista, cabe inteiro na área que rola: acima da
      // barra de ações e abaixo do cabeçalho.
      const area = rolagem.getBoundingClientRect();
      for (const id of controles) {
        const el = $(id);
        const alvo = el.closest("label") ?? el;
        alvo.scrollIntoView({ block: "nearest" });
        const c = alvo.getBoundingClientRect();
        if (c.height === 0) problemas.push(`${id} não está desenhado`);
        if (c.top < area.top - 0.5 || c.bottom > area.bottom + 0.5) {
          problemas.push(`${id} não cabe na área que rola (${Math.round(c.top)}–${Math.round(c.bottom)} em ${Math.round(area.top)}–${Math.round(area.bottom)})`);
        }
      }
      return problemas;
    }, controles);

    const retratar = async (page, nome, viewport) => {
      if (!pasta) return;
      await page.screenshot({ path: path.join(pasta, `${nome}-${viewport.width}x${viewport.height}.png`) });
    };

    for (const viewport of JANELAS) {
      const tamanho = `${viewport.width}x${viewport.height}`;
      const { page, erros, console_ } = await abrir(viewport);
      await preparar(page);

      // 1. O passo pergunta, com um grupo de opções de verdade.
      const grupo = await page.evaluate(() => {
        const grupo = document.querySelector("#tela-preparar [role=radiogroup]");
        const radios = [...grupo.querySelectorAll("input[type=radio]")];
        return {
          nome: document.getElementById(grupo.getAttribute("aria-labelledby"))?.textContent.trim(),
          nomes: radios.map((r) => r.name),
          rotulos: radios.map((r) => r.closest("label").querySelector(".preparar-porta-nome").textContent),
          marcado: radios.find((r) => r.checked)?.id,
          notas: radios.map((r) => document.getElementById(r.getAttribute("aria-describedby"))?.textContent ?? ""),
          campos: !$("preparar-senha-campos").hidden,
          regra: $("preparar-senha-regra").textContent,
        };
      });
      assert.equal(grupo.nome, "Quem pode entrar", `${tamanho}: o grupo de opções não tem nome`);
      assert.deepEqual(grupo.nomes, ["preparar-porta", "preparar-porta"], `${tamanho}: os rádios não são do mesmo grupo`);
      assert.deepEqual(grupo.rotulos, ["COM A SUA APROVAÇÃO", "COM SENHA"]);
      assert.equal(grupo.marcado, "preparar-porta-portao", `${tamanho}: o portão não vem marcado`);
      assert.match(grupo.notas[0], /aprova cada pessoa/, `${tamanho}: a linha do portão não diz o que ele é: ${grupo.notas[0]}`);
      assert.match(grupo.notas[1], /entra direto/, `${tamanho}: a linha da senha não diz o que ela é: ${grupo.notas[1]}`);
      assert.equal(grupo.campos, false, `${tamanho}: os campos da senha aparecem sem COM SENHA escolhido`);
      assert.match(grupo.regra, /Pelo menos 8 caracteres/, `${tamanho}: a regra não é dita antes de digitar: ${grupo.regra}`);

      // 2. Nada fora da janela, no portão.
      assert.deepEqual(await fora(page, ["preparar-porta-portao", "preparar-porta-senha"]), [], `${tamanho}, no portão`);
      await page.evaluate(() => $("preparar-porta-portao").scrollIntoView({ block: "center" }));
      await retratar(page, "portao", viewport);

      // 3. O teclado: a seta troca a escolha, e o Tab leva ao campo com rótulo.
      await page.focus("#preparar-porta-portao");
      await page.keyboard.press("ArrowDown");
      assert.equal(await page.evaluate(() => $("preparar-porta-senha").checked && document.activeElement.id === "preparar-porta-senha"), true,
        `${tamanho}: a seta não levou a escolha a COM SENHA`);
      assert.equal(await page.evaluate(() => !$("preparar-senha-campos").hidden), true, `${tamanho}: COM SENHA não mostrou os campos`);
      await page.keyboard.press("Tab");
      const campo = await page.evaluate(() => ({
        foco: document.activeElement.id,
        tipo: document.activeElement.type,
        rotulo: document.activeElement.labels?.[0]?.querySelector(".rotulo")?.textContent,
        descrito: document.activeElement.getAttribute("aria-describedby"),
      }));
      assert.deepEqual(campo, { foco: "preparar-senha", tipo: "password", rotulo: "SENHA DO SERVIDOR", descrito: "preparar-senha-regra preparar-senha-erro" },
        `${tamanho}: o Tab depois do grupo não caiu no campo da senha, com rótulo`);
      assert.equal(await page.evaluate(() => $("preparar-senha-de-novo").labels?.[0]?.querySelector(".rotulo")?.textContent), "REPITA A SENHA");
      assert.deepEqual(await fora(page, ["preparar-porta-portao", "preparar-porta-senha", "preparar-senha", "preparar-senha-de-novo", "preparar-senha-mostrar", "preparar-senha-regra"]),
        [], `${tamanho}, com senha`);

      // 5. As recusas: ao lado do campo, com o foco nele, e nada sobe.
      const recusar = async (senha, deNovo, frase, foco) => {
        await esquecer(page);
        await page.fill("#preparar-senha", senha);
        await page.fill("#preparar-senha-de-novo", deNovo);
        await page.focus("#preparar-senha");
        // O pior caso de uma janela baixa: o campo é a última coisa à vista, e
        // a recusa nasce logo abaixo dele, debaixo da barra de ações.
        await page.evaluate(() => $("preparar-senha").closest("label").scrollIntoView({ block: "end" }));
        await page.keyboard.press("Enter");
        await page.waitForFunction(() => !$("preparar-senha-erro").hidden);
        const visto = await page.evaluate(() => {
          // A recusa e o campo a que o foco voltou, os dois inteiros na área
          // que rola: acima da barra de ações, sem ninguém rolar.
          const area = document.querySelector("#tela-preparar .area-conteudo").getBoundingClientRect();
          const dentro = (el) => {
            const c = el.getBoundingClientRect();
            return c.height > 0 && c.top >= area.top - 0.5 && c.bottom <= area.bottom + 0.5;
          };
          return {
            erro: $("preparar-senha-erro").textContent,
            papel: $("preparar-senha-erro").getAttribute("role"),
            foco: document.activeElement.id,
            tela: !$("tela-preparar").hidden,
            aVista: dentro($("preparar-senha-erro")) && dentro(document.activeElement),
          };
        });
        assert.match(visto.erro, frase, `${tamanho}: a recusa de «${senha}» disse outra coisa: ${visto.erro}`);
        assert.equal(visto.papel, "alert");
        assert.equal(visto.foco, foco, `${tamanho}: o foco não voltou ao campo recusado`);
        assert.equal(visto.tela, true, `${tamanho}: a recusa saiu da tela de preparar`);
        assert.equal(visto.aVista, true, `${tamanho}: a recusa de «${senha}» (ou o campo dela) ficou fora da área à vista`);
        assert.deepEqual(await pedidos(page, ["criar_servidor", "hospedar"]), [], `${tamanho}: a senha recusada criou ou subiu um servidor`);
      };
      await recusar("", "", /DIGITE A SENHA DO SERVIDOR/, "preparar-senha");
      await retratar(page, "recusa-vazia", viewport);
      await recusar("     ", "     ", /DIGITE A SENHA DO SERVIDOR/, "preparar-senha");
      await recusar(` ${SENHA}`, ` ${SENHA}`, /COMEÇA OU TERMINA COM ESPAÇO/, "preparar-senha");
      await recusar("1234", "1234", /CURTA DEMAIS[\s\S]*pelo menos 8 caracteres/, "preparar-senha");
      await retratar(page, "recusa-curta", viewport);
      await recusar("é".repeat(65), "é".repeat(65), /LONGA DEMAIS/, "preparar-senha");
      await recusar(SENHA, `${SENHA}x`, /NÃO SÃO IGUAIS/, "preparar-senha-de-novo");
      // A recusa some quando a pessoa volta a digitar.
      await page.fill("#preparar-senha-de-novo", SENHA);
      assert.equal(await page.evaluate(() => $("preparar-senha-erro").hidden), true, `${tamanho}: a recusa ficou na tela depois de corrigida`);

      // 6. MOSTRAR e OCULTAR.
      await page.fill("#preparar-senha", SENHA);
      await page.click("#preparar-senha-mostrar");
      assert.deepEqual(await page.evaluate(() => [$("preparar-senha").type, $("preparar-senha-de-novo").type, $("preparar-senha-mostrar").textContent, $("preparar-senha-mostrar").getAttribute("aria-pressed")]),
        ["text", "text", "OCULTAR", "true"], `${tamanho}: MOSTRAR não mostrou`);
      assert.deepEqual(await fora(page, ["preparar-senha", "preparar-senha-de-novo", "preparar-senha-mostrar", "preparar-senha-regra"]), [], `${tamanho}, mostrando`);
      await page.evaluate(() => $("preparar-senha-regra").scrollIntoView({ block: "end" }));
      await retratar(page, "senha", viewport);
      await page.click("#preparar-senha-mostrar");
      assert.deepEqual(await page.evaluate(() => [$("preparar-senha").type, $("preparar-senha-de-novo").type, $("preparar-senha-mostrar").textContent, $("preparar-senha-mostrar").getAttribute("aria-pressed")]),
        ["password", "password", "MOSTRAR", "false"], `${tamanho}: OCULTAR não ocultou`);

      // 3 (o resto). Esc volta, e a senha não fica nos campos.
      await page.focus("#preparar-senha");
      await page.keyboard.press("Escape");
      assert.deepEqual(await page.evaluate(() => [$("tela-preparar").hidden, $("tela-boot").hidden, $("preparar-senha").value, $("preparar-senha-de-novo").value]),
        [true, false, "", ""], `${tamanho}: Esc não voltou à entrada com os campos vazios`);
      assert.deepEqual(await pedidos(page, ["criar_servidor", "hospedar"]), [], `${tamanho}: Esc hospedou`);

      // E a tela reaberta volta ao portão.
      await preparar(page);
      assert.deepEqual(await page.evaluate(() => [$("preparar-porta-portao").checked, $("preparar-senha-campos").hidden]), [true, true],
        `${tamanho}: a tela reaberta não voltou ao portão`);

      // 7. Com senha: o `hospedar` leva a senha, depois do `criar_servidor`.
      await esquecer(page);
      await page.check("#preparar-porta-senha");
      await page.fill("#preparar-nome", "Casa do Lago");
      await page.fill("#preparar-senha", SENHA);
      await page.fill("#preparar-senha-de-novo", SENHA);
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => chamadas.some((c) => c.cmd === "connect"));
      const subida = await pedidos(page, ["criar_servidor", "hospedar"]);
      assert.deepEqual(subida.map((c) => c.cmd), ["criar_servidor", "hospedar"], `${tamanho}: a ordem não foi criar e hospedar: ${JSON.stringify(subida.map((c) => c.cmd))}`);
      assert.deepEqual(subida[1].args, { nomePublico: null, servidor: "srv-novo", senha: SENHA }, `${tamanho}: o hospedar não levou a senha escolhida`);
      assert.deepEqual(await page.evaluate(() => [$("preparar-senha").value, $("preparar-senha-de-novo").value, $("preparar-senha").type]), ["", "", "password"],
        `${tamanho}: a senha ficou nos campos depois de hospedar`);

      // 8. A senha não vai a mais nenhum lugar.
      await page.waitForTimeout(300);
      const vazou = await page.evaluate((senha) => {
        const onde = [];
        for (const c of chamadas) {
          const args = { ...(c.args ?? {}) };
          if (c.cmd === "hospedar") delete args.senha;
          if (JSON.stringify(args).includes(senha)) onde.push(`o comando ${c.cmd}`);
        }
        if (document.documentElement.outerHTML.includes(senha)) onde.push("o HTML da página");
        for (const el of document.querySelectorAll("*")) {
          for (const atributo of el.attributes) {
            if (atributo.value.includes(senha)) onde.push(`o atributo ${atributo.name} de ${el.id || el.tagName}`);
          }
        }
        for (const guarda of [localStorage, sessionStorage]) {
          for (let i = 0; i < guarda.length; i += 1) {
            if ((guarda.getItem(guarda.key(i)) ?? "").includes(senha)) onde.push(`o armazenamento (${guarda.key(i)})`);
          }
        }
        return onde;
      }, SENHA);
      assert.deepEqual(vazou, [], `${tamanho}: a senha foi parar em: ${vazou.join(", ")}`);
      assert.deepEqual(console_.filter((linha) => linha.includes(SENHA)), [], `${tamanho}: a senha foi ao console`);
      assert.deepEqual(erros, [], `${tamanho}: erro de página`);
      await page.close();
    }

    // 4. Com aprovação, Enter no nome hospeda, e o `hospedar` vai sem senha.
    {
      const { page, erros } = await abrir(JANELAS[1]);
      await preparar(page);
      await page.fill("#preparar-nome", "Casa do Lago");
      await page.focus("#preparar-nome");
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => chamadas.some((c) => c.cmd === "connect"));
      const subida = await pedidos(page, ["criar_servidor", "hospedar"]);
      assert.deepEqual(subida.map((c) => c.cmd), ["criar_servidor", "hospedar"]);
      assert.deepEqual(subida[1].args, { nomePublico: null, servidor: "srv-novo", senha: null }, "no portão, o hospedar levou uma senha");
      assert.deepEqual(erros, []);
      await page.close();
    }

    // 5 (o número). O mínimo da recusa é o que a ponte manda, e não um daqui.
    {
      const { page, erros } = await abrir(JANELAS[1], { regras_da_senha_do_server: { minimo: 11, maximo_em_bytes: 128 } });
      await preparar(page);
      await page.check("#preparar-porta-senha");
      assert.match(await page.evaluate(() => $("preparar-senha-regra").textContent), /Pelo menos 11 caracteres/);
      await page.fill("#preparar-senha", "dezletras!");
      await page.fill("#preparar-senha-de-novo", "dezletras!");
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => !$("preparar-senha-erro").hidden);
      assert.match(await page.evaluate(() => $("preparar-senha-erro").textContent), /pelo menos 11 caracteres/);
      assert.deepEqual(await pedidos(page, ["criar_servidor", "hospedar"]), []);
      assert.deepEqual(erros, []);
      await page.close();
    }

    // 9. O servidor não subiu: a frase diz que a senha não foi gravada. E só
    //    quando havia senha.
    for (const comSenha of [true, false]) {
      const { page, erros } = await abrir(JANELAS[1], {}, { hospedar: "PortaOcupada" });
      await preparar(page);
      if (comSenha) {
        await page.check("#preparar-porta-senha");
        await page.fill("#preparar-senha", SENHA);
        await page.fill("#preparar-senha-de-novo", SENHA);
      }
      await page.click("#preparar-confirmar");
      await page.waitForFunction(() => !$("boot-erro").hidden && $("boot-erro").textContent !== "");
      const frase = await page.evaluate(() => $("boot-erro").textContent);
      assert.match(frase, /A PORTA 8383 JÁ ESTÁ EM USO/);
      assert.equal(/A senha escolhida não foi gravada/.test(frase), comSenha,
        `a frase ${comSenha ? "não diz" : "diz"} que a senha não foi gravada: ${frase}`);
      assert.equal(frase.includes(SENHA), false, "a frase da falha mostra a senha");
      assert.deepEqual(erros, []);
      await page.close();
    }

    // E a recusa do Rust tem frase, sem a senha dentro.
    {
      const { page, erros } = await abrir(JANELAS[1]);
      const frases = await page.evaluate(() => ["Vazia", "ComEspacoNaPonta", "Curta", "Longa"].map((r) => fraseDeErro({ SenhaRecusada: r }))
        .concat(fraseDeErro("PortaJaEscolhida")));
      for (const frase of frases) assert.match(frase, /SENHA/, `uma recusa do Rust ficou sem frase: ${frase}`);
      assert.equal(new Set(frases).size, frases.length, "duas recusas diferentes dizem a mesma frase");
      assert.deepEqual(erros, []);
      await page.close();
    }

    // 10. Quem adota o servidor que já estava aqui não ganha a pergunta, e o
    //     `hospedar` dele vai sem senha.
    for (const viewport of JANELAS) {
      const { page, erros } = await abrir(viewport, { servidores_guardados: [{ id: "principal", nome: "", versao: "0.15.1", caminho: null, ultimo_uso: 1 }] });
      await page.evaluate(() => abrirPreparar("principal"));
      await page.waitForFunction(() => !$("tela-preparar").hidden && !$("preparar-versoes").hidden);
      const adotado = await page.evaluate(() => {
        const cartao = document.querySelector("#tela-preparar .preparar-porta").getBoundingClientRect();
        return { escondido: $("preparar-do-novo").hidden, altura: cartao.height };
      });
      assert.deepEqual(adotado, { escondido: true, altura: 0 }, "quem adota um servidor ganhou a pergunta de um servidor novo");
      await retratar(page, "adotado", viewport);
      await page.click("#preparar-confirmar");
      await page.waitForFunction(() => chamadas.some((c) => c.cmd === "hospedar"));
      const subida = await pedidos(page, ["criar_servidor", "hospedar"]);
      assert.deepEqual(subida.map((c) => [c.cmd, c.args?.servidor, c.args?.senha]), [["hospedar", "principal", null]]);
      assert.deepEqual(erros, []);
      await page.close();
    }
  } finally {
    await browser.close();
    server.close();
  }
  console.log("a porta ao hospedar: o passo pergunta (portão marcado), a senha é conferida antes de criar, vai só ao hospedar, e nada sai da janela em 720×480, 900×600 e 1600×919.");
})().catch((e) => { console.error(e); process.exit(1); });
