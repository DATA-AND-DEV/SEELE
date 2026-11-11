// **A tela do aceite da máquina de jogo, a faixa com DESLIGAR e a lista com
// REVOGAR** (API 8, parte 3; `camada-maquina.js`).
//
// A casca roda no navegador, com a ponte do Tauri simulada (`invoke` anota cada
// chamada e responde o que a bancada combinou, `listen` guarda os ouvintes para
// a bancada emitir `maquina-pedido` e `maquina-estado`). Só o que está do outro
// lado da ponte é de mentira: a tela, a faixa, a fila, o teclado e a lista de
// aceites são os de verdade.
//
// O que se prova (os sete casos do plano, na ordem, e depois os que seguram o
// que a tela promete):
//
//  1. `maquina-pedido` sem termos: título, MOD, jogo, porta, «nada para baixar»,
//     «nenhum programa», disco com a pasta e o aviso da conta; ACEITAR chama
//     `maquina_aceitar` com o `resumo` do evento;
//  2. com dois termos, ACEITAR fica desligado até as duas caixas;
//  3. RECUSAR (e Esc) chama `maquina_recusar`, e nunca `maquina_aceitar`;
//  4. `maquina-estado { ligada: true }`: a faixa mostra a linha; DESLIGAR chama
//     `maquina_desligar` com as três chaves; `ligada: false, motivo: "pelo-dono"`:
//     a linha diz «desligado por você»;
//  5. `mudou: ["versao_do_jogo", "rodar"]`: a tela diz o que mudou, em frase;
//  6. dois pedidos seguidos: um por vez, o segundo depois de responder o primeiro;
//  7. na lista de aceites, REVOGAR chama `maquina_revogar`;
//  8. o aviso da conta está à vista sem rolar, mesmo numa janela baixa e com um
//     pedido enorme;
//  9. o que o MOD escolheu entra como texto, e nunca como marcação;
// 10. clicar no véu não responde nada;
// 11. a falha do aceite aparece na tela, em frase, e a tela continua lá;
// 12. um pedido que o servidor desfez sai da tela, e a faixa diz por quê, com o
//     payload exato que o Rust emite (`EstadoNaTela`: alvo, mod_id, servidor,
//     ligada:false, motivo), e o mesmo para `outro-pedido`;
// 12b. o aviso sem `resumo` (máquina que estava de pé) não fecha o diálogo de um
//     pedido novo da mesma chave; só o que nomeia o resumo dele fecha;
// 13. a máquina ligada continua com DESLIGAR à vista e clicável quando a janela
//     está na tela de entrada, e não só na da sessão;
// 14. na lista de aceites, uma máquina ligada tem DESLIGAR ao lado de REVOGAR;
// 15. (parte 4) cada artefato deste computador, com nome, origem e tamanho; a
//     soma só deles; os de outros sistemas ditos e fora da soma; e as origens
//     de onde mais arquivos podem vir.
// 16. (parte 6) o pack pelo hash, o instalador do loader antes do servidor, e
//     os domínios de onde o loader baixa sozinho.
// 17. (revisão final, M9) a falha de `maquinas_aceitas` aparece na lista, em
//     frase; e (M2, no caso 1) cada porta vem com o nome do túnel.
// 18. (Windows, W1) só os comandos deste computador, pelo `por_sistema`, e
//     quantos outros sistemas têm comandos próprios.
// 19. (e2e de 31/10, passo 13) a faixa diz «serve… o jogo está rodando aqui»
//     só com o jogo de pé (`maquina-jogo`), e «pronto… não está rodando aqui»
//     com a metade de pé e o jogo parado; a lista relida também.
// 20. (e2e de 31/10, passo 14) o jogo que sobrou de um app que fechou sem
//     desligá-lo: `maquina-sobra` e `maquinas_sobras` põem na faixa o que se
//     achou e o que se fez, uma linha por processo; e o que não parou manda
//     encerrá-lo pela ferramenta do sistema em que a janela roda.
// 21. (e2e de 31/10, passo 5) o MOD sem o id repetido, e o servidor do SEELE
//     pelo nome, e não o id do jogo.
// 22. (API 8 parte 7) o pedido da reserva diz que é a reserva — chip e título à
//     vista sem rolar, e a linha do que isso quer dizer —, e o da principal não;
//     `mudou: ["papel"]` tem frase. (O 4b prova também que «o jogo segue
//     rodando aqui» só sai com o jogo de pé.)
//
//   PLAYWRIGHT=/caminho/para/node_modules/playwright node apps/seele-app/bancada/aceite-da-maquina.cjs

const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");
const engines = require("./playwright.cjs");

/** O que falhou, por motor e caso: um caso que falha não esconde os outros. */
const falhas = [];
async function caso(motor, numero, prova) {
  try {
    await prova();
  } catch (erro) {
    // **A mensagem inteira, numa linha só.** Cortada na primeira quebra, a
    // reprovação do caso 20 mostrava só a primeira linha da faixa (a do
    // processo 4242), e dali se leu que a linha do 4646 não existia: ela
    // estava três linhas abaixo, com a frase certa. A faixa tem uma linha por
    // máquina, então toda mensagem que a cita tem quebras.
    falhas.push(`${motor} · caso ${numero}: ${String(erro.message).trim().split(/\n+/).join(" ⏎ ")}`);
  }
}

(async () => {
  const servidor = servir();
  await new Promise((ok) => servidor.listen(0, "127.0.0.1", ok));
  try {
    for (const motor of ["chromium", "webkit"]) {
      const navegador = await engines[motor]().launch({ headless: true });
      try {
        const pagina = await navegador.newPage({ viewport: { width: 1400, height: 900 } });
        const erros = [];
        pagina.on("pageerror", (e) => erros.push(e.message));
        await pagina.addInitScript((tabela) => {
          window.table = tabela;
          window.chamadas = [];
          window.ouvintes = [];
          window.__TAURI__ = {
            core: { invoke: async (cmd, args) => {
              chamadas.push({ cmd, args });
              const r = table[cmd];
              return typeof r === "function" ? r(args) : (r ?? null);
            } },
            event: { listen: async (nome, fn) => { ouvintes.push({ nome, fn }); return () => {}; } },
            window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
          };
        }, respostas({
          snapshot: null, amigos: [], contatos_da_agenda: [], microfones: [], saidas: [],
          pacotes_no_cache: [], aceites_de_mods: [], plataforma: "macos", maquinas_aceitas: [],
        }));
        await pagina.goto(`http://127.0.0.1:${servidor.address().port}`);
        await pagina.waitForFunction(() => !conferindoMods);

        // A sessão desenhada: é nela que mora a faixa, como a das portas.
        await pagina.evaluate((r) => {
          table.snapshot = r;
          $("tela-boot").hidden = true;
          $("tela-sessao").hidden = false;
          desenhar(r);
          pedirAoServidor = async () => ({ ok: true, conjunto: "conj-1", mods: [] });
          entrarNaGeracao(1);
        }, retrato());
        await pagina.waitForFunction(() => !conferindoMods);

        await pagina.evaluate(() => {
          window.emitir = (nome, payload) => ouvintes.filter((o) => o.nome === nome).forEach((o) => o.fn({ payload }));
          // O pedido que o Rust emite, com o que cada caso muda.
          // `artefatos_daqui` e `de_outros_sistemas` como o Rust os calcula
          // (`artefatos_desta_maquina`), para um Mac ARM: sem marca, serve
          // aqui; com marca, só se ela nomeia este sistema.
          window.pedido = (extra = {}, oferta = {}) => {
            const o = {
              titulo: "Minecraft", receita: "vanilla", versao_da_receita: "1.2", versao_do_jogo: "1.21",
              tuneis: [{ nome: "minecraft-1", protocolo: "tcp", porta: 25565 }],
              artefatos: [], pack: null, origens: [], dominios_do_jogo: [], rodar: null,
              disco_maximo: 2 * 1024 * 1024 * 1024, termos: [],
              ...oferta,
            };
            const daqui = o.artefatos.filter((a) => !(a.plataformas ?? []).length || a.plataformas.includes("mac-aarch64"));
            // E os comandos como o Rust os escolhe (`comandos_desta_maquina`):
            // a entrada de `por_sistema` deste sistema, ou o par de cima.
            const sistemas = o.por_sistema ?? [];
            const doSistema = sistemas.find((e) => e.plataformas.includes("mac-aarch64"));
            const comandos = {
              rodar_daqui: doSistema ? doSistema.rodar ?? null : o.rodar ?? null,
              instalar_daqui: doSistema ? doSistema.instalar ?? null : o.instalar ?? null,
              sistemas_com_outros_comandos: sistemas.flatMap((e) => e.plataformas).filter((p) => p !== "mac-aarch64").length,
            };
            return {
              alvo: "127.0.0.1:7000", mod_id: "x/jogo", mod_nome: "x/jogo", servidor: "srv-1",
              servidor_seele: "Casa E2E", pedido_por: "Fulano", resumo: "resumo-1", pasta: "/home/eu/seele/jogos/x-jogo", mudou: null,
              oferta: o, artefatos_daqui: daqui, de_outros_sistemas: o.artefatos.length - daqui.length,
              ...comandos,
              ...extra,
            };
          };
          window.chamadasDe = (cmd) => chamadas.filter((c) => c.cmd === cmd).map((c) => c.args);
          window.tela = () => $("maquina-aceite").hidden ? "" : $("maquina-aceite").innerText;
          window.faixaDeMaquinas = () => $("maquinas").hidden ? "" : $("maquinas").innerText;
          table.maquina_aceitar = () => ({});
          table.maquina_recusar = () => null;
          table.maquina_desligar = () => null;
          table.maquina_revogar = () => null;
        });
        const zerar = () => pagina.evaluate(() => {
          fecharOPainelDeJogos();
          chamadas.length = 0;
          // Fecha o que a prova anterior deixou, pelo caminho do produto.
          filaDeMaquinas.length = 0;
          if (!$("maquina-aceite").hidden) fecharATela();
          maquinasNaFaixa.clear();
          jogosRodando.clear();
          desenharAFaixaDeMaquinas();
        });
        const emitirPedido = (extra, oferta) => pagina.evaluate(([e, o]) => emitir("maquina-pedido", pedido(e, o)), [extra, oferta]);
        const aberta = () => pagina.evaluate(() => !$("maquina-aceite").hidden);
        const esperarTela = (tituloContem) => pagina.waitForFunction(
          (t) => !$("maquina-aceite").hidden && $("maquina-titulo").innerText.includes(t), tituloContem, { timeout: 5000 },
        );
        const esperarFechada = () => pagina.waitForFunction(() => $("maquina-aceite").hidden, null, { timeout: 5000 });

        // 1. Sem termos: tudo o que a pessoa precisa ler; ACEITAR leva o resumo.
        await caso(motor, "1", async () => {
          await zerar();
          await emitirPedido();
          await esperarTela("Fulano");
          const texto = await pagina.evaluate(() => tela());
          assert.match(texto, /Fulano pede para rodar um servidor de jogo neste computador/, `${motor}: 1 · título: ${texto}`);
          assert.match(texto, /MOD: x\/jogo(?! \()/, `${motor}: 1 · ${texto}`);
          assert.match(texto, /jogo: Minecraft, versão 1\.21/, `${motor}: 1 · ${texto}`);
          assert.match(texto, /porta 25565 \(tcp\)/, `${motor}: 1 · ${texto}`);
          // M2: o nome do túnel, para a cópia do mundo não passar por porta do jogo.
          assert.match(texto, /o túnel «minecraft-1» escuta na porta 25565/, `${motor}: 1 · ${texto}`);
          assert.match(texto, /nada para baixar neste pedido/, `${motor}: 1 · ${texto}`);
          assert.match(texto, /nenhum programa: este pedido só abre a porta do jogo/, `${motor}: 1 · ${texto}`);
          assert.match(texto, /ocupa até 2\.0 GB de disco, em \/home\/eu\/seele\/jogos\/x-jogo/, `${motor}: 1 · ${texto}`);
          assert.match(texto, /o servidor do jogo roda com a sua conta deste computador, e pode ler o que ela lê\./, `${motor}: 1 · sem o aviso da conta: ${texto}`);
          assert.match(texto, /você desliga quando quiser, pela faixa no alto da tela/, `${motor}: 1 · ${texto}`);
          assert.doesNotMatch(texto, /quem aceita os termos/, `${motor}: 1 · sem termos, não há quem os aceite: ${texto}`);
          assert.equal(await pagina.evaluate(() => $("maquina-aceitar").disabled), false, `${motor}: 1 · ACEITAR desligado sem termos`);
          await pagina.click("#maquina-aceitar");
          await esperarFechada();
          assert.deepEqual(await pagina.evaluate(() => chamadasDe("maquina_aceitar")), [
            { alvo: "127.0.0.1:7000", modId: "x/jogo", servidor: "srv-1", resumo: "resumo-1" },
          ], `${motor}: 1 · as chaves de maquina_aceitar`);
          assert.equal(await pagina.evaluate(() => chamadasDe("maquina_recusar").length), 0, `${motor}: 1 · recusou ao aceitar`);
        });

        // 2. Com dois termos, ACEITAR só liga com as duas caixas.
        await caso(motor, "2", async () => {
          await zerar();
          await emitirPedido({}, { termos: [
            { titulo: "EULA do jogo", url: "https://exemplo.com/eula" },
            { titulo: "Política de dados", url: "https://exemplo.com/dados" },
          ] });
          await esperarTela("Fulano");
          const disabled = () => pagina.evaluate(() => $("maquina-aceitar").disabled);
          const caixas = pagina.locator(".maquina-termo-caixa");
          assert.equal(await caixas.count(), 2, `${motor}: 2 · caixas`);
          assert.equal(await disabled(), true, `${motor}: 2 · ACEITAR ligado sem nenhuma caixa`);
          const texto = await pagina.evaluate(() => tela());
          assert.match(texto, /li e aceito: EULA do jogo/, `${motor}: 2 · ${texto}`);
          assert.match(texto, /quem aceita os termos do jogo é você, e não o SEELE nem o MOD/, `${motor}: 2 · ${texto}`);
          await caixas.nth(0).check();
          assert.equal(await disabled(), true, `${motor}: 2 · ACEITAR ligou com uma caixa só`);
          await caixas.nth(1).check();
          assert.equal(await disabled(), false, `${motor}: 2 · ACEITAR não ligou com as duas`);
          await caixas.nth(0).uncheck();
          assert.equal(await disabled(), true, `${motor}: 2 · ACEITAR ficou ligado com uma caixa desmarcada`);
          // E o clique no desligado não envia nada.
          await pagina.evaluate(() => $("maquina-aceitar").click());
          assert.equal(await pagina.evaluate(() => chamadasDe("maquina_aceitar").length), 0, `${motor}: 2 · aceitou com um termo por marcar`);
          // O endereço do termo está escrito, e LER o abre fora da janela.
          assert.match(texto, /https:\/\/exemplo\.com\/eula/, `${motor}: 2 · o endereço do termo não aparece`);
          await pagina.locator(".maquina-ler").nth(0).click();
          assert.deepEqual(await pagina.evaluate(() => chamadasDe("abrir_no_navegador")), [{ url: "https://exemplo.com/eula" }], `${motor}: 2 · LER`);
        });

        // 3. RECUSAR e Esc: maquina_recusar, e nunca maquina_aceitar.
        await caso(motor, "3", async () => {
          await zerar();
          await emitirPedido();
          await esperarTela("Fulano");
          assert.equal(await pagina.evaluate(() => document.activeElement?.id), "maquina-recusar", `${motor}: 3 · o foco não nasce no RECUSAR`);
          await pagina.click("#maquina-recusar");
          await esperarFechada();
          const esperado = [{ alvo: "127.0.0.1:7000", modId: "x/jogo", servidor: "srv-1" }];
          assert.deepEqual(await pagina.evaluate(() => chamadasDe("maquina_recusar")), esperado, `${motor}: 3 · RECUSAR`);
          await emitirPedido();
          await esperarTela("Fulano");
          await pagina.keyboard.press("Escape");
          await esperarFechada();
          assert.deepEqual(await pagina.evaluate(() => chamadasDe("maquina_recusar")), [...esperado, ...esperado], `${motor}: 3 · Esc`);
          assert.equal(await pagina.evaluate(() => chamadasDe("maquina_aceitar").length), 0, `${motor}: 3 · aceitou ao recusar`);
        });

        // 4. A faixa: a linha, DESLIGAR, e a parada em frase.
        await caso(motor, "4", async () => {
          await zerar();
          await pagina.evaluate(() => {
            titulosDeMaquinas.set(chaveDaMaquina("127.0.0.1:7000", "x/jogo", "srv-1"), "Minecraft");
            emitir("maquina-estado", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", ligada: true });
          });
          const faixa = await pagina.evaluate(() => faixaDeMaquinas());
          assert.match(faixa, /este computador está pronto para servir Minecraft \(x\/jogo\)/, `${motor}: 4 · ${faixa}`);
          assert.match(faixa, /DESLIGAR/, `${motor}: 4 · ${faixa}`);
          // Na sessão, a faixa mora no painel do selo, na barra de baixo (0.19.2).
          await pagina.click(".selo-de-jogo");
          await pagina.click(".maquina-desligar");
          assert.deepEqual(await pagina.evaluate(() => chamadasDe("maquina_desligar")), [
            { alvo: "127.0.0.1:7000", modId: "x/jogo", servidor: "srv-1" },
          ], `${motor}: 4 · as três chaves de maquina_desligar`);
          // Só o Rust diz que parou: a linha não muda antes do evento.
          assert.match(await pagina.evaluate(() => faixaDeMaquinas()), /este computador está pronto/, `${motor}: 4 · a faixa mudou antes do evento`);
          await pagina.evaluate(() => emitir("maquina-estado", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", ligada: false, motivo: "pelo-dono" }));
          const depois = await pagina.evaluate(() => faixaDeMaquinas());
          assert.match(depois, /Minecraft \(x\/jogo\): desligado por você/, `${motor}: 4 · ${depois}`);
          assert.doesNotMatch(depois, /DESLIGAR/, `${motor}: 4 · a parada ainda oferece DESLIGAR`);
          // O motivo novo do Rust tem frase.
          await pagina.evaluate(() => emitir("maquina-estado", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", ligada: false, motivo: "pacote-diferente" }));
          assert.match(await pagina.evaluate(() => faixaDeMaquinas()), /o pacote do MOD não confere com o que você aceitou/, `${motor}: 4 · pacote-diferente`);
        });

        // 4b. Sem quem hospeda: a faixa segue de pé, em laranja, e diz até
        // quando; a órfã que ninguém chamou de volta para, em vermelho.
        await caso(motor, "4b", async () => {
          await zerar();
          await pagina.evaluate(() => {
            titulosDeMaquinas.set(chaveDaMaquina("127.0.0.1:7000", "x/jogo", "srv-1"), "Minecraft");
            emitir("maquina-estado", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", ligada: true, motivo: "sem-anfitriao" });
          });
          let faixa = await pagina.evaluate(() => faixaDeMaquinas());
          assert.match(faixa, /este computador está pronto para servir Minecraft/, `${motor}: 4b · ${faixa}`);
          // **Sem jogo de pé, nada «segue rodando»**: a faixa dizia as duas
          // coisas ao mesmo tempo («não está rodando» e «segue rodando aqui»).
          assert.doesNotMatch(faixa, /segue rodando aqui/, `${motor}: 4b · «segue rodando» sem jogo: ${faixa}`);
          assert.match(faixa, /quem hospeda o SEELE saiu, e o jogo não está rodando aqui; se ele não chamar esta máquina de volta em 10 minutos, ela é desligada/, `${motor}: 4b · sem jogo: ${faixa}`);
          assert.match(faixa, /DESLIGAR/, `${motor}: 4b · a órfã não oferece DESLIGAR`);
          // Com o jogo de pé, ele segue rodando, e quem joga pelo direto também.
          await pagina.evaluate(() => emitir("maquina-jogo", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", rodando: true }));
          faixa = await pagina.evaluate(() => faixaDeMaquinas());
          assert.match(faixa, /quem hospeda o SEELE saiu: o jogo segue rodando aqui/, `${motor}: 4b · com jogo: ${faixa}`);
          assert.doesNotMatch(faixa, /ela é desligada/, `${motor}: 4b · com jogo: ${faixa}`);
          const cor = await pagina.evaluate(() => getComputedStyle(document.querySelector(".maquina-bloco")).borderTopColor);
          assert.notEqual(cor, "rgba(0, 0, 0, 0)", `${motor}: 4b · sem borda`);
          if (process.env.SEELE_RETRATO_DA_FAIXA) await pagina.screenshot({ path: `${process.env.SEELE_RETRATO_DA_FAIXA}-${motor}-orfa.png` });
          // A volta religa: a faixa volta ao verde, sem o aviso.
          await pagina.evaluate(() => emitir("maquina-estado", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", ligada: true }));
          assert.doesNotMatch(await pagina.evaluate(() => faixaDeMaquinas()), /quem hospeda o SEELE saiu/, `${motor}: 4b · o aviso ficou depois da volta`);
          await pagina.evaluate(() => emitir("maquina-estado", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", ligada: false, motivo: "sem-volta" }));
          assert.match(await pagina.evaluate(() => faixaDeMaquinas()), /não chamou esta máquina de volta em 10 minutos; o jogo foi salvo e parado/, `${motor}: 4b · sem-volta`);
          assert.equal(await pagina.evaluate(() => document.querySelector(".maquina-bloco").dataset.motivo), "sem-volta");
          if (process.env.SEELE_RETRATO_DA_FAIXA) await pagina.screenshot({ path: `${process.env.SEELE_RETRATO_DA_FAIXA}-${motor}-sem-volta.png` });
        });

        // 5. O que mudou desde o aceite, em frase.
        await caso(motor, "5", async () => {
          await zerar();
          await emitirPedido({ mudou: ["versao_do_jogo", "rodar", "pacote"] });
          await esperarTela("Fulano");
          const texto = await pagina.evaluate(() => tela());
          assert.match(
            texto,
            /isto mudou desde o seu aceite: a versão do jogo, o programa e os argumentos, a versão do MOD\. Por isso o SEELE pergunta de novo\./,
            `${motor}: 5 · ${texto}`,
          );
          await zerar();
          await emitirPedido();
          await esperarTela("Fulano");
          assert.doesNotMatch(await pagina.evaluate(() => tela()), /isto mudou/, `${motor}: 5 · a primeira vez diz que algo mudou`);
        });

        // 6. Dois pedidos: um por vez.
        await caso(motor, "6", async () => {
          await zerar();
          await emitirPedido({ pedido_por: "Primeira", servidor: "srv-1" });
          await emitirPedido({ pedido_por: "Segunda", servidor: "srv-2", resumo: "resumo-2" });
          await esperarTela("Primeira");
          let texto = await pagina.evaluate(() => tela());
          assert.doesNotMatch(texto, /SEGUNDA/, `${motor}: 6 · o segundo pedido apareceu junto do primeiro: ${texto}`);
          assert.equal(await pagina.evaluate(() => document.querySelectorAll("[id='maquina-aceite']:not([hidden])").length), 1, `${motor}: 6 · duas telas`);
          await pagina.click("#maquina-aceitar");
          await esperarTela("Segunda");
          texto = await pagina.evaluate(() => tela());
          assert.doesNotMatch(texto, /PRIMEIRA/, `${motor}: 6 · ${texto}`);
          await pagina.click("#maquina-aceitar");
          await esperarFechada();
          assert.deepEqual(
            await pagina.evaluate(() => chamadasDe("maquina_aceitar").map((a) => a.resumo)),
            ["resumo-1", "resumo-2"],
            `${motor}: 6 · a ordem das respostas`,
          );
        });

        // 7. A lista de aceites: REVOGAR.
        await caso(motor, "7", async () => {
          await zerar();
          await pagina.evaluate(() => {
            let revogada = false;
            table.maquinas_aceitas = () => revogada ? [] : [{
              alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", titulo: "Minecraft",
              pedido_por: "Fulano", quando: 1, ligada: true, tuneis: [{ nome: "minecraft-1", protocolo: "tcp", porta: 25565 }],
            }];
            table.maquina_revogar = () => { revogada = true; return null; };
          });
          await pagina.evaluate(() => desenharMaquinasAceitas());
          const lista = await pagina.evaluate(() => $("lista-maquinas-aceitas").innerText);
          assert.match(lista, /Minecraft \(x\/jogo\) · servidor srv-1/, `${motor}: 7 · ${lista}`);
          assert.match(lista, /ligada agora/, `${motor}: 7 · ${lista}`);
          assert.match(lista, /REVOGAR/, `${motor}: 7 · ${lista}`);
          await pagina.evaluate(() => {
            const botao = [...$("lista-maquinas-aceitas").querySelectorAll("button")].find((b) => b.textContent === "REVOGAR");
            botao.click();
          });
          await pagina.waitForFunction(() => /NENHUM/.test($("lista-maquinas-aceitas").innerText), null, { timeout: 5000 });
          assert.deepEqual(await pagina.evaluate(() => chamadasDe("maquina_revogar")), [
            { alvo: "127.0.0.1:7000", modId: "x/jogo", servidor: "srv-1" },
          ], `${motor}: 7 · as três chaves de maquina_revogar`);
        });

        // 8. O aviso da conta, sem rolar, numa janela baixa e com um pedido enorme.
        await caso(motor, "8", async () => {
          await zerar();
          await pagina.setViewportSize({ width: 1400, height: 420 });
          try {
            const muitos = (n, f) => Array.from({ length: n }, (_, i) => f(i));
            await emitirPedido({}, {
              tuneis: muitos(8, (i) => ({ nome: `t${i}`, protocolo: "tcp", porta: 25560 + i })),
              artefatos: muitos(30, (i) => ({ nome: `a${i}.jar`, url: `https://cdn.exemplo.com/a${i}.jar`, tamanho: 1048576, hash: { algoritmo: "sha256", hex: "ab" } })),
              origens: ["cdn.exemplo.com"],
              dominios_do_jogo: muitos(10, (i) => `d${i}.exemplo.com`),
              rodar: { comando: "java", argumentos: muitos(40, (i) => `-Dopcao${i}=valor`) },
              termos: muitos(6, (i) => ({ titulo: `Termo ${i}`, url: `https://exemplo.com/t${i}` })),
            });
            await esperarTela("Fulano");
            const medida = await pagina.evaluate(() => {
              const r = $("maquina-sua-conta").getBoundingClientRect();
              const corpo = $("maquina-corpo");
              const acoes = $("maquina-aceitar").getBoundingClientRect();
              const topo = $("maquina-titulo").getBoundingClientRect();
              return {
                topo: r.top, base: r.bottom, janela: innerHeight, largura: r.width,
                rola: corpo.scrollHeight > corpo.clientHeight, corpoScroll: corpo.scrollTop,
                aceitarBase: acoes.bottom, tituloTopo: topo.top,
                dentro: $("maquina-sua-conta").closest("#maquina-corpo") !== null,
              };
            });
            assert.ok(medida.rola, `${motor}: 8 · o pedido não era grande o bastante para rolar: ${JSON.stringify(medida)}`);
            assert.ok(!medida.dentro, `${motor}: 8 · o aviso está dentro da área que rola`);
            assert.ok(medida.topo >= 0 && medida.base <= medida.janela && medida.largura > 0, `${motor}: 8 · o aviso da conta não está à vista sem rolar: ${JSON.stringify(medida)}`);
            assert.ok(medida.aceitarBase <= medida.janela && medida.tituloTopo >= 0, `${motor}: 8 · título ou botões fora da janela: ${JSON.stringify(medida)}`);
            // E depois de rolar o corpo até o fim, o aviso não saiu do lugar.
            await pagina.evaluate(() => { $("maquina-corpo").scrollTop = 1e6; });
            const depois = await pagina.evaluate(() => $("maquina-sua-conta").getBoundingClientRect().top);
            assert.equal(Math.round(depois), Math.round(medida.topo), `${motor}: 8 · o aviso andou com a rolagem`);
          } finally {
            await pagina.setViewportSize({ width: 1400, height: 900 });
          }
        });

        // 9. O que o MOD escolheu é texto, e nunca marcação.
        await caso(motor, "9", async () => {
          await zerar();
          await pagina.evaluate(() => { window.xssFoi = false; });
          const isca = "<img src=x onerror=\"window.xssFoi=true\">";
          await emitirPedido({ pedido_por: isca, mod_id: isca }, {
            titulo: isca,
            rodar: { comando: isca, argumentos: [isca] },
            termos: [{ titulo: isca, url: "https://exemplo.com/" }],
          });
          await esperarTela("pede para rodar");
          assert.equal(await pagina.locator("#maquina-aceite img").count(), 0, `${motor}: 9 · virou marcação`);
          assert.equal(await pagina.evaluate(() => window.xssFoi), false, `${motor}: 9 · o texto do MOD rodou`);
          assert.ok((await pagina.evaluate(() => tela())).includes(isca), `${motor}: 9 · o texto não aparece como texto`);
        });

        // 10. Clicar no véu não é resposta.
        await caso(motor, "10", async () => {
          await zerar();
          await emitirPedido();
          await esperarTela("Fulano");
          await pagina.mouse.click(5, 5);
          assert.equal(await aberta(), true, `${motor}: 10 · a tela fechou por fora`);
          assert.equal(await pagina.evaluate(() => chamadas.filter((c) => c.cmd.startsWith("maquina_")).length), 0, `${motor}: 10 · o clique no véu respondeu`);
        });

        // 11. A falha do aceite aparece, em frase, e a tela continua.
        await caso(motor, "11", async () => {
          await zerar();
          await pagina.evaluate(() => { table.maquina_aceitar = () => { throw "conteudo-nao-bate"; }; });
          await emitirPedido();
          await esperarTela("Fulano");
          await pagina.click("#maquina-aceitar");
          await pagina.waitForFunction(() => !$("maquina-erro").hidden, null, { timeout: 5000 });
          const erro = await pagina.evaluate(() => $("maquina-erro").textContent);
          assert.match(erro, /o pacote do MOD não confere com o que o servidor anunciou; nada foi ligado/, `${motor}: 11 · ${erro}`);
          assert.equal(await aberta(), true, `${motor}: 11 · a tela fechou como se tivesse dado certo`);
          assert.equal(await pagina.evaluate(() => $("maquina-recusar").disabled), false, `${motor}: 11 · RECUSAR ficou preso`);
          await pagina.evaluate(() => { table.maquina_aceitar = () => ({}); });
        });

        // 12. O servidor desfez o pedido: a tela sai, e a faixa diz por quê. O
        // payload é o que `dizer_pedidos_desfeitos` emite no Rust, sem mais nem menos.
        await caso(motor, "12", async () => {
          for (const [motivo, frase] of [["servidor-saiu", /você saiu do servidor/], ["outro-pedido", /o servidor passou este jogo para outra máquina/]]) {
            await zerar();
            await emitirPedido();
            await esperarTela("Fulano");
            await pagina.evaluate((m) => {
              titulosDeMaquinas.set(chaveDaMaquina("127.0.0.1:7000", "x/jogo", "srv-1"), "Minecraft");
              emitir("maquina-estado", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", ligada: false, motivo: m, resumo: "resumo-1" });
            }, motivo);
            await esperarFechada();
            assert.match(await pagina.evaluate(() => faixaDeMaquinas()), frase, `${motor}: 12 · ${motivo}: a faixa não diz por quê`);
          }
        });

        // 12b. O aviso de uma máquina que estava de pé (sem `resumo`) não fecha o
        // diálogo de um pedido NOVO da mesma chave; o que nomeia o resumo, fecha.
        await caso(motor, "12b", async () => {
          await zerar();
          await emitirPedido({ resumo: "R2" });
          await esperarTela("Fulano");
          await pagina.evaluate(() => emitir("maquina-estado", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", ligada: false, motivo: "outro-pedido" }));
          assert.equal(await aberta(), true, `${motor}: 12b · um aviso sem resumo fechou o diálogo do pedido novo`);
          await pagina.evaluate(() => emitir("maquina-estado", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", ligada: false, motivo: "outro-pedido", resumo: "R1" }));
          assert.equal(await aberta(), true, `${motor}: 12b · o resumo de outro pedido fechou o diálogo`);
          await pagina.evaluate(() => emitir("maquina-estado", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", ligada: false, motivo: "outro-pedido", resumo: "R2" }));
          await esperarFechada();
        });

        // 13. A faixa é global: na tela de entrada, DESLIGAR segue à vista e funciona.
        await caso(motor, "13", async () => {
          await zerar();
          await pagina.evaluate(() => {
            titulosDeMaquinas.set(chaveDaMaquina("127.0.0.1:7000", "x/jogo", "srv-1"), "Minecraft");
            emitir("maquina-estado", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", ligada: true });
            $("tela-sessao").hidden = true;
            $("tela-boot").hidden = false;
          });
          try {
            const medida = await pagina.evaluate(() => {
              const b = document.querySelector(".maquina-desligar");
              const r = b.getBoundingClientRect();
              const no = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
              return { w: r.width, h: r.height, topo: r.top, base: r.bottom, janela: innerHeight, alcanca: no === b || b.contains(no), tela: $("tela-boot").hidden };
            });
            assert.equal(medida.tela, false, `${motor}: 13 · a tela de entrada não está à vista`);
            assert.ok(medida.w > 0 && medida.h > 0 && medida.topo >= 0 && medida.base <= medida.janela, `${motor}: 13 · DESLIGAR fora da janela: ${JSON.stringify(medida)}`);
            assert.ok(medida.alcanca, `${motor}: 13 · algo cobre o DESLIGAR: ${JSON.stringify(medida)}`);
            await pagina.click(".maquina-desligar");
            assert.deepEqual(await pagina.evaluate(() => chamadasDe("maquina_desligar")), [
              { alvo: "127.0.0.1:7000", modId: "x/jogo", servidor: "srv-1" },
            ], `${motor}: 13 · DESLIGAR na tela de entrada`);
          } finally {
            await pagina.evaluate(() => { $("tela-boot").hidden = true; $("tela-sessao").hidden = false; });
          }
        });

        // 14. A lista de aceites: DESLIGAR ao lado de REVOGAR, só para a que está ligada.
        await caso(motor, "14", async () => {
          await zerar();
          await pagina.evaluate(() => {
            table.maquinas_aceitas = () => [
              { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", titulo: "Minecraft", pedido_por: "Fulano", quando: 1, ligada: true, tuneis: [] },
              { alvo: "127.0.0.1:7000", mod_id: "x/outro", servidor: "srv-1", titulo: "Terraria", pedido_por: "Fulano", quando: 1, ligada: false, tuneis: [] },
            ];
          });
          await pagina.evaluate(() => desenharMaquinasAceitas());
          const rotulos = await pagina.evaluate(() => [...$("lista-maquinas-aceitas").querySelectorAll("li")].map((li) => [...li.querySelectorAll("button")].map((b) => b.textContent)));
          assert.deepEqual(rotulos, [["DESLIGAR", "REVOGAR"], ["REVOGAR"]], `${motor}: 14 · botões da lista`);
          await pagina.evaluate(() => [...$("lista-maquinas-aceitas").querySelectorAll("button")].find((b) => b.textContent === "DESLIGAR").click());
          await pagina.waitForFunction(() => chamadas.some((c) => c.cmd === "maquina_desligar"), null, { timeout: 5000 });
          assert.deepEqual(await pagina.evaluate(() => chamadasDe("maquina_desligar")), [
            { alvo: "127.0.0.1:7000", modId: "x/jogo", servidor: "srv-1" },
          ], `${motor}: 14 · as três chaves de maquina_desligar pela lista`);
        });

        // 15. Cada artefato deste computador, com nome, origem e tamanho; os
        //     de outros sistemas fora da soma, e ditos; as origens livres, ditas.
        await caso(motor, "15", async () => {
          await zerar();
          const java = (p, mb) => ({
            nome: `java-${p}`, url: `https://github.com/adoptium/${p}.tar.gz`, tamanho: mb * 1048576,
            hash: { algoritmo: "sha256", hex: "ab" }, plataformas: [p],
          });
          await emitirPedido({}, {
            artefatos: [
              { nome: "server.jar", url: "https://piston-data.mojang.com/v1/objects/x/server.jar", tamanho: 55 * 1048576, hash: { algoritmo: "sha1", hex: "ab" } },
              java("windows-x64", 47), java("mac-aarch64", 50), java("mac-x64", 51), java("linux-x64", 52), java("linux-aarch64", 53),
            ],
            origens: ["github.com", "release-assets.githubusercontent.com"],
          });
          await esperarTela("Fulano");
          const linhas = await pagina.evaluate(() => [...$("maquina-detalhes").children].map((li) => li.innerText));
          const de = (re) => linhas.filter((l) => re.test(l));
          assert.deepEqual(de(/vai baixar/), ["vai baixar 2 arquivos, 105.0 MB no total:"], `${motor}: 15 · a soma: ${linhas.join(" | ")}`);
          assert.deepEqual(de(/^· /), [
            "· server.jar, de piston-data.mojang.com, 55.0 MB",
            "· java-mac-aarch64, de github.com, 50.0 MB",
          ], `${motor}: 15 · um por artefato: ${linhas.join(" | ")}`);
          assert.deepEqual(de(/outros sistemas/), ["4 arquivos são de outros sistemas, e este computador não os baixa"], `${motor}: 15 · ${linhas.join(" | ")}`);
          assert.deepEqual(de(/pode baixar outros/), [
            "e pode baixar outros arquivos, cada um com o hash conferido, de github.com, release-assets.githubusercontent.com",
          ], `${motor}: 15 · ${linhas.join(" | ")}`);
        });

        // 16. (parte 6) o modpack: o pack pelo hash, os dois comandos — o
        //     instalador do loader antes do servidor — e os domínios de onde o
        //     loader baixa sozinho, ditos como «o jogo baixa por conta própria».
        await caso(motor, "16", async () => {
          await zerar();
          await emitirPedido({}, {
            pack: { algoritmo: "sha512", hex: "57764c105ff999ced4c7e2069e36dcf2".repeat(4) },
            instalar: { comando: "java/bin/java", argumentos: ["-jar", "fabric-installer.jar", "server", "-mcversion", "1.21.4"] },
            rodar: { comando: "java/bin/java", argumentos: ["-jar", "fabric-server-launch.jar", "nogui"] },
            dominios_do_jogo: ["maven.fabricmc.net", "meta.fabricmc.net"],
          });
          await esperarTela("Fulano");
          const linhas = await pagina.evaluate(() => [...$("maquina-detalhes").children].map((li) => li.innerText));
          const de = (re) => linhas.filter((l) => re.test(l));
          assert.deepEqual(de(/pack do modpack/), ["o pack do modpack, conferido pelo sha512 57764c105ff999ce…"], `${motor}: 16 · o pack: ${linhas.join(" | ")}`);
          assert.deepEqual(de(/vai rodar/), [
            "antes, para instalar, vai rodar: java/bin/java -jar fabric-installer.jar server -mcversion 1.21.4",
            "vai rodar: java/bin/java -jar fabric-server-launch.jar nogui",
          ], `${motor}: 16 · os dois comandos: ${linhas.join(" | ")}`);
          assert.deepEqual(de(/por conta própria/), [
            "o jogo baixa por conta própria de maven.fabricmc.net, meta.fabricmc.net, sem o SEELE conferir",
          ], `${motor}: 16 · ${linhas.join(" | ")}`);
        });

        // 18. (Windows, decisão W1) a oferta do Forge traz o `unix_args.txt`
        //     em cima e o `win_args.txt` na entrada do Windows: a tela mostra
        //     só o par deste computador (o Rust escolhe; a bancada, como um
        //     Mac ARM e depois como um Windows), e diz que outro sistema tem
        //     comandos próprios. O `rodar` cru da oferta não aparece.
        await caso(motor, "18", async () => {
          const unix = { comando: "java/bin/java", argumentos: ["@libraries/net/minecraftforge/forge/1.20.1-47.4.28/unix_args.txt", "nogui"] };
          const win = { comando: "java/bin/java.exe", argumentos: ["@libraries/net/minecraftforge/forge/1.20.1-47.4.28/win_args.txt", "nogui"] };
          const instalarWin = { comando: "java/bin/java.exe", argumentos: ["-jar", "forge-installer.jar", "--installServer"] };
          const oferta = {
            rodar: unix,
            instalar: { comando: "java/bin/java", argumentos: ["-jar", "forge-installer.jar", "--installServer"] },
            por_sistema: [{ plataformas: ["windows-x64"], rodar: win, instalar: instalarWin }],
          };
          const ler = async () => pagina.evaluate(() => [...$("maquina-detalhes").children].map((li) => li.innerText));
          await zerar();
          await emitirPedido({}, oferta);
          await esperarTela("Fulano");
          let linhas = await ler();
          assert.deepEqual(linhas.filter((l) => /vai rodar/.test(l)), [
            "antes, para instalar, vai rodar: java/bin/java -jar forge-installer.jar --installServer",
            "vai rodar: java/bin/java @libraries/net/minecraftforge/forge/1.20.1-47.4.28/unix_args.txt nogui",
          ], `${motor}: 18 · no Mac: ${linhas.join(" | ")}`);
          assert.ok(linhas.includes("outro sistema tem comandos próprios, que não rodam aqui"), `${motor}: 18 · ${linhas.join(" | ")}`);
          await zerar();
          await emitirPedido({ rodar_daqui: win, instalar_daqui: instalarWin, sistemas_com_outros_comandos: 0 }, oferta);
          await esperarTela("Fulano");
          linhas = await ler();
          assert.deepEqual(linhas.filter((l) => /vai rodar/.test(l)), [
            "antes, para instalar, vai rodar: java/bin/java.exe -jar forge-installer.jar --installServer",
            "vai rodar: java/bin/java.exe @libraries/net/minecraftforge/forge/1.20.1-47.4.28/win_args.txt nogui",
          ], `${motor}: 18 · no Windows: ${linhas.join(" | ")}`);
          assert.ok(!linhas.some((l) => l.includes("unix_args")), `${motor}: 18 · o unix_args apareceu no Windows`);
          assert.ok(!linhas.some((l) => /comandos próprios/.test(l)), `${motor}: 18 · ${linhas.join(" | ")}`);
        });

        // 17. (revisão final, M9) `maquinas_aceitas` que falha: a lista diz
        //     a falha em frase, em vez de ficar vazia e calada.
        await caso(motor, "17", async () => {
          await zerar();
          await pagina.evaluate(() => {
            $("maquinas-aceitas-erro").hidden = true;
            table.maquinas_aceitas = () => { throw { Recusado: { motivo: "aceite-invalido" } }; };
          });
          await pagina.evaluate(() => desenharMaquinasAceitas());
          const erro = await pagina.evaluate(() => ({ hidden: $("maquinas-aceitas-erro").hidden, texto: $("maquinas-aceitas-erro").innerText }));
          assert.equal(erro.hidden, false, `${motor}: 17 · a falha não apareceu`);
          assert.ok(erro.texto.length > 0, `${motor}: 17 · a falha apareceu sem frase`);
          await pagina.evaluate(() => { table.maquinas_aceitas = () => []; $("maquinas-aceitas-erro").hidden = true; });
        });

        // 19. A faixa diz se o jogo roda, e não só que a metade vive.
        await caso(motor, "19", async () => {
          await zerar();
          const chaves = { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1" };
          await pagina.evaluate((c) => {
            titulosDeMaquinas.set(chaveDaMaquina(c.alvo, c.mod_id, c.servidor), "Minecraft");
            emitir("maquina-estado", { ...c, ligada: true });
          }, chaves);
          let faixa = await pagina.evaluate(() => faixaDeMaquinas());
          assert.match(faixa, /pronto para servir Minecraft \(x\/jogo\), e o jogo não está rodando aqui/, `${motor}: 19 · sem jogo: ${faixa}`);
          assert.doesNotMatch(faixa, /este computador serve/, `${motor}: 19 · «serve» sem jogo: ${faixa}`);
          await pagina.evaluate((c) => emitir("maquina-jogo", { ...c, rodando: true }), chaves);
          faixa = await pagina.evaluate(() => faixaDeMaquinas());
          assert.match(faixa, /este computador serve Minecraft \(x\/jogo\): o jogo está rodando aqui/, `${motor}: 19 · com jogo: ${faixa}`);
          // O jogo caiu (ou passou à reserva): a metade segue, e a faixa diz.
          await pagina.evaluate((c) => emitir("maquina-jogo", { ...c, rodando: false }), chaves);
          faixa = await pagina.evaluate(() => faixaDeMaquinas());
          assert.match(faixa, /o jogo não está rodando aqui/, `${motor}: 19 · depois de o jogo acabar: ${faixa}`);
          assert.match(faixa, /DESLIGAR/, `${motor}: 19 · ${faixa}`);
          // A janela que recarrega lê `jogo` da lista de aceites.
          await zerar();
          await pagina.evaluate(async (c) => {
            table.maquinas_aceitas = () => [{ ...c, titulo: "Minecraft", pedido_por: "Fulano", quando: 1, ligada: true, jogo: true, tuneis: [] }];
            await relerAsMaquinasLigadas();
          }, chaves);
          faixa = await pagina.evaluate(() => faixaDeMaquinas());
          assert.match(faixa, /este computador serve Minecraft \(x\/jogo\): o jogo está rodando aqui/, `${motor}: 19 · relida: ${faixa}`);
          await pagina.evaluate(() => { table.maquinas_aceitas = () => []; });
        });

        // 20. O jogo que sobrou de um app que fechou sem desligá-lo.
        await caso(motor, "20", async () => {
          await zerar();
          const sobra = { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", titulo: "Minecraft 26.3", pid: 4242, desfecho: "saiu", segundos: 1.94 };
          await pagina.evaluate((s) => emitir("maquina-sobra", s), sobra);
          let faixa = await pagina.evaluate(() => faixaDeMaquinas());
          assert.match(faixa, /Minecraft 26\.3 \(x\/jogo\) seguia rodando neste computador \(processo 4242\) desde que o SEELE fechou sem desligá-lo/, `${motor}: 20 · ${faixa}`);
          assert.match(faixa, /pediu ao jogo para salvar e sair, e ele saiu em 1,9 s/, `${motor}: 20 · ${faixa}`);
          // A mesma sobra pelo comando (a janela que abriu depois) não repete a linha.
          await pagina.evaluate(async (s) => { table.maquinas_sobras = () => [s, { ...s, pid: 4343, desfecho: "forcada", segundos: 20 }]; for (const x of await invoke("maquinas_sobras")) aplicarASobra(x); }, sobra);
          faixa = await pagina.evaluate(() => faixaDeMaquinas());
          assert.equal((faixa.match(/processo 4242/g) ?? []).length, 1, `${motor}: 20 · linha repetida: ${faixa}`);
          assert.match(faixa, /processo 4343.*não saiu em 20,0 s e foi encerrado à força/s, `${motor}: 20 · forçada: ${faixa}`);
          assert.equal(await pagina.evaluate(() => document.querySelector('.maquina-bloco[data-motivo="sobra-forcada"]') !== null), true, `${motor}: 20 · a forçada não está em alerta`);
          // Ainda salvando (revisão da 0.19, I1): a faixa diz que nenhum jogo
          // sobe até ele sair, e o desfecho toma o lugar dessa linha.
          await pagina.evaluate((s) => emitir("maquina-sobra", { ...s, pid: 4545, desfecho: "parando", segundos: 0 }), sobra);
          faixa = await pagina.evaluate(() => faixaDeMaquinas());
          assert.match(faixa, /processo 4545.*espera ele sair; até lá, nenhum jogo sobe neste computador/s, `${motor}: 20 · parando: ${faixa}`);
          await pagina.evaluate((s) => emitir("maquina-sobra", { ...s, pid: 4545, desfecho: "saiu", segundos: 3 }), sobra);
          faixa = await pagina.evaluate(() => faixaDeMaquinas());
          assert.equal((faixa.match(/processo 4545/g) ?? []).length, 1, `${motor}: 20 · o parando ficou ao lado do desfecho: ${faixa}`);
          assert.doesNotMatch(faixa, /nenhum jogo sobe/, `${motor}: 20 · o parando não saiu: ${faixa}`);
          // Que não parou: a única sobra que pede uma ação de quem lê. O
          // payload é o do Rust (`sobra_na_tela`: `nao-parou`, zero segundos e
          // o `motivo`). O caminho vale fora do Mac também (revisão da 0.19,
          // M4) — e, desde a 0.19.5, a frase nomeia a ferramenta do sistema em
          // que a janela roda (`ambienteDasInstrucoes`), no lugar de «o
          // gerenciador de processos do sistema (no Mac, o Monitor de
          // Atividade)» para todos. Esta bancada ficou esperando a frase
          // antiga num Mac, e reprovava com o produto certo: agora prova a de
          // cada sistema, e que nenhuma cita a ferramenta de outro.
          await pagina.evaluate((s) => emitir("maquina-sobra", { ...s, pid: 4646, desfecho: "nao-parou", segundos: 0, motivo: "EPERM" }), sobra);
          const ferramentas = { macos: "Monitor de Atividade", windows: "Gerenciador de Tarefas", outro: "gerenciador de processos do sistema" };
          const plataformaDaBancada = await pagina.evaluate(() => $("barra-janela").dataset.plataforma);
          assert.equal(plataformaDaBancada, "macos", `${motor}: 20 · a bancada combina um Mac, e a barra diz ${plataformaDaBancada}`);
          try {
            for (const [plataforma, ferramenta] of Object.entries(ferramentas)) {
              faixa = await pagina.evaluate((p) => {
                $("barra-janela").dataset.plataforma = p;
                desenharAFaixaDeMaquinas();
                return faixaDeMaquinas();
              }, plataforma);
              const linhas = faixa.split("\n").filter((l) => l.includes("processo 4646"));
              assert.equal(linhas.length, 1, `${motor}: 20 · nao-parou (${plataforma}): a faixa não tem uma linha, e uma só, do processo 4646: ${faixa}`);
              assert.match(linhas[0], /o SEELE não conseguiu pará-lo \(EPERM\): ele segue rodando, com o mundo e a porta/, `${motor}: 20 · nao-parou (${plataforma}): ${linhas[0]}`);
              assert.ok(linhas[0].endsWith(`encerre-o pelo ${ferramenta}.`), `${motor}: 20 · nao-parou (${plataforma}): a linha não manda ao ${ferramenta}: ${linhas[0]}`);
              for (const [outra, alheia] of Object.entries(ferramentas)) {
                if (outra !== plataforma) assert.ok(!linhas[0].includes(alheia), `${motor}: 20 · nao-parou (${plataforma}): a linha cita a ferramenta de ${outra}: ${linhas[0]}`);
              }
            }
          } finally {
            // A barra volta ao sistema combinado: os casos seguintes leem dela.
            await pagina.evaluate((p) => { $("barra-janela").dataset.plataforma = p; desenharAFaixaDeMaquinas(); }, plataformaDaBancada);
          }
          await pagina.evaluate(() => { table.maquinas_sobras = () => []; });
        });

        // 21. O MOD sem o id repetido; o servidor do SEELE pelo nome.
        await caso(motor, "21", async () => {
          await zerar();
          await emitirPedido();
          await esperarTela("Fulano");
          let texto = await pagina.evaluate(() => tela());
          assert.doesNotMatch(texto, /x\/jogo \(x\/jogo\)/, `${motor}: 21 · id repetido: ${texto}`);
          assert.match(texto, /pedido por Fulano, no servidor do SEELE «Casa E2E»/, `${motor}: 21 · ${texto}`);
          assert.doesNotMatch(texto, /no servidor srv-1/, `${motor}: 21 · o id do jogo como servidor: ${texto}`);
          await pagina.click("#maquina-recusar");
          await esperarFechada();
          // Com um nome de exibição, o id vai entre parênteses.
          await emitirPedido({ mod_nome: "Multiplayer" });
          await esperarTela("Fulano");
          texto = await pagina.evaluate(() => tela());
          assert.match(texto, /MOD: Multiplayer \(x\/jogo\)/, `${motor}: 21 · ${texto}`);
          await pagina.click("#maquina-recusar");
          await esperarFechada();
        });

        // 22. (API 8 parte 7) O pedido da reserva diz que é a reserva: no
        //     alto, sem rolar (chip e título), e numa linha que diz o que isso
        //     quer dizer. O da principal não diz nada disso.
        await caso(motor, "22", async () => {
          await zerar();
          await emitirPedido({ papel: "reserva", resumo: "resumo-reserva" });
          await esperarTela("Fulano");
          const texto = await pagina.evaluate(() => tela());
          assert.match(texto, /Fulano pede que este computador seja a reserva de um servidor de jogo/, `${motor}: 22 · título: ${texto}`);
          assert.match(texto, /como reserva: este computador só roda o jogo se a máquina principal sair, com uma cópia do mundo/, `${motor}: 22 · ${texto}`);
          const chip = await pagina.evaluate(() => {
            const c = $("maquina-papel");
            const r = c.getBoundingClientRect();
            const corpo = $("maquina-corpo").getBoundingClientRect();
            return { texto: c.innerText, visivel: r.width > 0 && r.height > 0, acimaDoCorpo: r.bottom <= corpo.top };
          });
          assert.equal(chip.texto, "RESERVA", `${motor}: 22 · chip: ${JSON.stringify(chip)}`);
          assert.ok(chip.visivel && chip.acimaDoCorpo, `${motor}: 22 · o chip não está à vista sem rolar: ${JSON.stringify(chip)}`);
          await pagina.click("#maquina-aceitar");
          await esperarFechada();
          assert.deepEqual(await pagina.evaluate(() => chamadasDe("maquina_aceitar")), [
            { alvo: "127.0.0.1:7000", modId: "x/jogo", servidor: "srv-1", resumo: "resumo-reserva" },
          ], `${motor}: 22 · o aceite da reserva leva o resumo dela`);
          // A principal: sem chip, sem a linha, o título de sempre.
          await emitirPedido({ papel: "principal" });
          await esperarTela("Fulano");
          const principal = await pagina.evaluate(() => ({ texto: tela(), chip: $("maquina-papel").getBoundingClientRect().width }));
          assert.doesNotMatch(principal.texto, /reserva/i, `${motor}: 22 · a principal falou de reserva: ${principal.texto}`);
          assert.equal(principal.chip, 0, `${motor}: 22 · chip na principal`);
          await pagina.click("#maquina-recusar");
          await esperarFechada();
          // O papel que mudou desde o aceite tem frase.
          await emitirPedido({ papel: "reserva", mudou: ["papel"] });
          await esperarTela("Fulano");
          assert.match(await pagina.evaluate(() => tela()), /isto mudou desde o seu aceite: o papel deste computador \(principal ou reserva\)/, `${motor}: 22 · mudou`);
          await pagina.click("#maquina-recusar");
          await esperarFechada();
        });

        // 23. (0.19.1) O pedido chega com um modal de MOD aberto — o caso real:
        //     o CRIAR do Multiplayer mora num diálogo do MOD. A pilha de camadas
        //     adormece (`inert`) tudo o que não é o modal, e a tela de aceite
        //     adormecia junto: aparecia, e a caixa do EULA e o ACEITAR não
        //     respondiam ao clique — só o Esc, que escuta o documento, recusava.
        await caso(motor, "23", async () => {
          await zerar();
          await pagina.evaluate(() => {
            const modal = document.createElement("div");
            modal.id = "modal-de-mod-de-bancada";
            modal.textContent = "diálogo do MOD";
            document.body.append(modal);
            empilharCamada(modal);
          });
          try {
            await emitirPedido({}, { termos: [{ titulo: "EULA do Minecraft (Mojang)", url: "https://aka.ms/MinecraftEULA" }] });
            await esperarTela("Fulano");
            assert.equal(await pagina.evaluate(() => $("maquina-aceite").closest("[inert]") === null), true,
              `${motor}: 23 · a tela de aceite ficou inerte sob o modal do MOD`);
            await pagina.locator(".maquina-termo-caixa").first().check({ timeout: 3000 });
            await pagina.click("#maquina-aceitar", { timeout: 3000 });
            await esperarFechada();
            assert.equal(await pagina.evaluate(() => chamadasDe("maquina_aceitar").length), 1, `${motor}: 23 · o aceite não chegou`);
            // Fechada a tela, o modal do MOD continua sendo o único acordado.
            assert.equal(await pagina.evaluate(() => $("maquina-aceite").closest("[inert]") !== null), true,
              `${motor}: 23 · a tela fechada não voltou a dormir sob o modal`);
            // E na ordem inversa: a tela já aberta, e um modal de MOD que entra depois.
            await pagina.evaluate(() => {
              const modal = document.getElementById("modal-de-mod-de-bancada");
              desempilharCamada(modal);
            });
            await emitirPedido({ resumo: "resumo-depois" }, { termos: [{ titulo: "EULA do Minecraft (Mojang)", url: "https://aka.ms/MinecraftEULA" }] });
            await esperarTela("Fulano");
            await pagina.evaluate(() => empilharCamada(document.getElementById("modal-de-mod-de-bancada")));
            await pagina.locator(".maquina-termo-caixa").first().check({ timeout: 3000 });
            await pagina.click("#maquina-aceitar", { timeout: 3000 });
            await esperarFechada();
            assert.equal(await pagina.evaluate(() => chamadasDe("maquina_aceitar").length), 2, `${motor}: 23 · o aceite com o modal aberto depois não chegou`);
          } finally {
            await pagina.evaluate(() => {
              const modal = document.getElementById("modal-de-mod-de-bancada");
              desempilharCamada(modal);
              modal.remove();
            });
          }
        });

        // 24. (0.19.2) O jogo na barra de baixo, e não no alto da sessão: o selo
        //     com o nome, o estado e o endereço; o painel com o detalhe e as
        //     ações acima da barra; e, fora da sessão, a faixa de cima de volta.
        await caso(motor, "24", async () => {
          await zerar();
          await emitirPedido({}, {});
          await esperarTela("Fulano");
          await pagina.click("#maquina-aceitar");
          await esperarFechada();
          await pagina.evaluate(() => emitir("maquina-estado", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", ligada: true }));
          const selo = await pagina.evaluate(() => {
            const s = document.querySelector("#tel-jogos .selo-de-jogo");
            const faixa = $("maquinas").getBoundingClientRect();
            return {
              texto: s?.innerText ?? null, tom: s?.dataset.tom ?? null,
              naBarra: Boolean(s?.closest("footer.telemetria")), visivel: Boolean(s && s.getBoundingClientRect().width > 0),
              faixaOcupa: Math.round(faixa.width * faixa.height),
            };
          });
          assert.equal(selo.texto, "SRV-1 · PRONTO · localhost:25565", `${motor}: 24 · o selo: ${JSON.stringify(selo)}`);
          assert.ok(selo.naBarra && selo.visivel && selo.tom === "pronto", `${motor}: 24 · o selo não está na barra de baixo: ${JSON.stringify(selo)}`);
          assert.ok(selo.faixaOcupa <= 1, `${motor}: 24 · a faixa de cima ainda ocupa a tela na sessão: ${JSON.stringify(selo)}`);
          if (process.env.SEELE_RETRATO_DA_FAIXA) await pagina.screenshot({ path: `${process.env.SEELE_RETRATO_DA_FAIXA}-${motor}-selo.png` });
          // O painel: acima da barra, com a faixa dentro, o endereço e o DESLIGAR.
          await pagina.click(".selo-de-jogo");
          if (process.env.SEELE_RETRATO_DA_FAIXA) await pagina.screenshot({ path: `${process.env.SEELE_RETRATO_DA_FAIXA}-${motor}-painel.png` });
          const painel = await pagina.evaluate(() => {
            const p = $("painel-jogos");
            return {
              aberto: !p.hidden, temAFaixa: $("maquinas").parentNode === p,
              endereco: p.querySelector(".maquina-endereco")?.value ?? null,
              acimaDaBarra: p.getBoundingClientRect().bottom <= document.querySelector("footer.telemetria").getBoundingClientRect().top,
              expandido: document.querySelector(".selo-de-jogo").getAttribute("aria-expanded"),
            };
          });
          assert.deepEqual(painel, { aberto: true, temAFaixa: true, endereco: "localhost:25565", acimaDaBarra: true, expandido: "true" },
            `${motor}: 24 · o painel: ${JSON.stringify(painel)}`);
          await pagina.click("#painel-jogos .maquina-copiar");
          assert.ok(await pagina.isVisible("#painel-jogos .maquina-desligar"), `${motor}: 24 · o DESLIGAR não está no painel`);
          // O jogo que começa muda o selo.
          await pagina.evaluate(() => emitir("maquina-jogo", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", rodando: true }));
          assert.equal(await pagina.evaluate(() => document.querySelector(".selo-de-jogo").innerText), "SRV-1 · RODANDO · localhost:25565", `${motor}: 24 · rodando`);
          // Esc fecha, e a faixa volta para a casa dela.
          await pagina.keyboard.press("Escape");
          assert.deepEqual(await pagina.evaluate(() => ({ aberto: !$("painel-jogos").hidden, naCasa: $("maquinas").parentNode === document.body })),
            { aberto: false, naCasa: true }, `${motor}: 24 · o Esc não fechou o painel, ou a faixa não voltou`);
          // Fora da sessão, a faixa de cima diz tudo de novo.
          const fora = await pagina.evaluate(async () => {
            $("tela-sessao").hidden = true;
            await new Promise((ok) => setTimeout(ok, 50));
            const r = $("maquinas").getBoundingClientRect();
            const medido = { modo: document.documentElement.dataset.jogosNaBarra, altura: Math.round(r.height), texto: $("maquinas").innerText };
            $("tela-sessao").hidden = false;
            await new Promise((ok) => setTimeout(ok, 50));
            return medido;
          });
          assert.ok(fora.modo === "nao" && fora.altura > 20 && /está pronto|rodando/.test(fora.texto),
            `${motor}: 24 · fora da sessão a faixa de cima não voltou: ${JSON.stringify(fora)}`);
          await pagina.evaluate(() => emitir("maquina-estado", { alvo: "127.0.0.1:7000", mod_id: "x/jogo", servidor: "srv-1", ligada: false, motivo: "pelo-dono" }));
        });

        await caso(motor, "página", async () => assert.deepEqual(erros, [], `${motor}: erros na página`));
        console.log(JSON.stringify({ motor, casos: 25, falhas: falhas.filter((f) => f.startsWith(motor)).length }));
        await pagina.close();
      } finally {
        await navegador.close();
      }
    }
  } finally {
    servidor.close();
  }
  if (falhas.length > 0) {
    console.error(`FALHOU:\n${falhas.join("\n")}`);
    process.exitCode = 1;
  }
})().catch((e) => { console.error(e); process.exitCode = 1; });
