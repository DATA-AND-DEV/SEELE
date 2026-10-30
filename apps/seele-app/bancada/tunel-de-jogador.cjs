// **A porta de quem joga abre de um clique, e o produto diz que abriu** (API 8,
// parte 2, `SeeleUI.tunel.entrar`).
//
// A casca roda no navegador, com a ponte do Tauri simulada (`invoke` anota cada
// chamada e responde o que a bancada combinou, `listen` guarda os ouvintes para
// a bancada emitir eventos). Só o que está do outro lado da ponte é de mentira:
// `atenderOMod`, o bilhete de gesto, o botão de verdade montado numa superfície,
// o fecho dos túneis no descarregar e a faixa do produto são os de verdade.
//
// O que se prova (os sete casos do plano, na ordem, e depois os da faixa):
//
//  1. o clique no `botao` do MOD leva o bilhete; `tunel-entrar` com ele chama
//     `tunel_entrar`, o MOD recebe a porta (25565), e a faixa do produto mostra
//     a porta e o nome;
//  2. `tunel-entrar` sem bilhete: `fora-de-um-clique`, e nenhum `invoke`;
//  3. o mesmo bilhete duas vezes: a segunda é recusada, e nenhum `invoke`;
//  4. o bilhete usado depois de 2 s (relógio de mentira): recusado — e o usado
//     dentro dos 2 s passa (o prazo é a fronteira, e não uma recusa geral);
//  5. a ponte responde a porta seguinte: a faixa diz que a do jogo estava
//     ocupada e qual usar;
//  6. desligar o MOD: `tunel_sair_do_mod` com o id, e a linha some da faixa;
//  7. um MOD sem `porta` que posta `tunel-entrar` direto: `tipo-desconhecido`,
//     e nenhum `invoke`;
//  8. o relatório de um túnel direto: «ligação direta com <apelido>», a
//     latência e a vazão;
//  9. a queda de um túnel pela estrela: a faixa diz «pela estrela» e a frase da
//     queda, que não acusa a rede;
// 10. o túnel que acabou: «fechou: …», com o motivo em frase e DISPENSAR;
// 11. a recusa do servidor chega ao MOD pelo nome E à faixa, em frase;
// 12. a faixa mora fora de toda superfície de MOD.
//
//   PLAYWRIGHT=/caminho/para/node_modules/playwright node apps/seele-app/bancada/tunel-de-jogador.cjs

const assert = require("node:assert/strict");
const { servir, respostas, retrato } = require("./telas.cjs");
const engines = require("./playwright.cjs");

/** O que falhou, por motor e caso: um caso que falha não esconde os outros. */
const falhas = [];
async function caso(motor, numero, prova) {
  try {
    await prova();
  } catch (erro) {
    falhas.push(`${motor} · caso ${numero}: ${String(erro.message).split("\n")[0]}`);
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
          pacotes_no_cache: [], aceites_de_mods: [], plataforma: "macos",
        }));
        await pagina.goto(`http://127.0.0.1:${servidor.address().port}`);
        await pagina.waitForFunction(() => !conferindoMods);

        // A sessão desenhada, e dois MODs da API 8 de pé: um com o poder
        // `porta`, e um sem.
        await pagina.evaluate((r) => {
          table.snapshot = r;
          $("tela-boot").hidden = true;
          $("tela-sessao").hidden = false;
          desenhar(r);
          // O catálogo é de mentira desde o primeiro instante: a conferência que
          // `entrarNaGeracao` dispara perguntaria ao servidor de verdade, e a
          // volta de quatro segundos não pode levar o MOD embora no meio.
          pedirAoServidor = async () => ({ ok: true, conjunto: "conj-1", mods: [] });
          entrarNaGeracao(1);
        }, retrato());
        await pagina.waitForFunction(() => !conferindoMods);

        await pagina.evaluate(() => {
          // A sessão 1 está na vista: é o carimbo que os MODs que sobem levam.
          porNaVista({ sessao: 1, geracao: 1 });
          window.entregues = [];
          window.paradas = 0;
          const levantar = (mod) => {
            const instancia = new InstanciaDeMod(mod.id, mod.hash, geracaoDaSessao, {
              pedirEncerramento() { paradas += 1; },
              async encerrou() { return true; },
              entregar(m) { entregues.push({ mod: mod.id, ...JSON.parse(JSON.stringify(m)) }); },
            });
            instancia.estado = ESTADOS_DE_MOD.ativa;
            modsCarregados.set(mod.id, instancia);
            // O mesmo fecho que `montarOMod` liga, e que o descarregar chama.
            ligarOFechoDosTuneis(mod, instancia);
            return { mod, instancia };
          };
          window.jogo = levantar({ id: "x/jogo", hash: "h-jogo", api: 8, poderes: ["porta"] });
          window.semPorta = levantar({ id: "x/sem-porta", hash: "h-sem", api: 8, poderes: [] });
          let n = 0;
          // O que o prelúdio posta, entregue ao roteador de verdade, e a
          // resposta que o MOD receberia.
          window.pedir = async (quem, mensagem) => {
            const numero = ++n;
            await atenderOMod(quem.mod, quem.instancia, { ...mensagem, n: numero });
            return entregues.find((m) => m.tipo === "resposta" && m.n === numero && m.mod === quem.mod.id) ?? null;
          };
          window.entradasNaPonte = () => chamadas.filter((c) => c.cmd === "tunel_entrar");
          window.gestoDoUltimoClique = () => entregues.filter((m) => m.mod === "x/jogo" && m.tipo === "evento" && m.nome === "botao").at(-1)?.gesto;
          window.emitir = (nome, payload) => ouvintes.filter((o) => o.nome === nome).forEach((o) => o.fn({ payload }));
          window.faixa = () => $("tuneis").hidden ? "" : $("tuneis").innerText;
          // O relógio de mentira: o deslocamento soma ao `performance.now`.
          window.deslocamento = 0;
          const real = performance.now.bind(performance);
          performance.now = () => real() + deslocamento;
          table.tunel_entrar = () => ({ porta: 25565, preferida: true, desejada: 25565 });
          table.tunel_sair_do_mod = () => null;
          table.tuneis_abertos = () => [];
          // Os dois MODs são do catálogo e estão instalados e carregados: a
          // volta de quatro segundos os deixa em paz.
          const catalogo = [jogo, semPorta].map(({ mod }) => ({ id: mod.id, version: "1.0.0", hash: mod.hash, plataformas: ["app"] }));
          table.mods_instalados = [jogo, semPorta].map(({ mod }) => ({ id: mod.id, hash: mod.hash, client: true, poderes: mod.poderes }));
          pedirAoServidor = async () => ({ ok: true, conjunto: "conj-1", mods: catalogo });
        });

        // O botão de verdade, montado numa superfície do MOD de jogo.
        const montou = await pagina.evaluate(async () => {
          const criou = await pedir(jogo, { tipo: "superficie-criar", descricao: { id: "mesa", tipo: "folha", titulo: "Jogar" } });
          const mostrou = await pedir(jogo, { tipo: "superficie-mostrar", superficie: "mesa" });
          const montada = await pedir(jogo, { tipo: "superficie-montar", superficie: "mesa", arvore: [{ forma: "botao", chave: "jogar", dentro: "JOGAR" }] });
          return { criou, mostrou, montada };
        });
        assert.equal(montou.montada?.ok, true, `${motor}: o botão não montou: ${JSON.stringify(montou)}`);
        const botao = '[data-superficie="mesa"] [data-chave-do-mod="jogar"]';
        const clicar = async () => {
          await pagina.click(botao);
          const gesto = await pagina.evaluate(() => gestoDoUltimoClique());
          assert.equal(typeof gesto, "string", `${motor}: o clique no botão do MOD não levou o bilhete`);
          return gesto;
        };
        const entrar = (nome, gesto) => pagina.evaluate(
          ([nome, gesto]) => pedir(jogo, { tipo: "tunel-entrar", nome, ...(gesto === undefined ? {} : { gesto }) }),
          [nome, gesto],
        );
        const entradas = () => pagina.evaluate(() => entradasNaPonte().length);

        // 1. O clique leva o bilhete; a porta abre; a faixa mostra porta e nome.
        await caso(motor, "1", async () => {
          const gesto = await clicar();
          const resposta = await entrar("minecraft-1", gesto);
          assert.equal(resposta?.ok, true, `${motor}: 1 · ${JSON.stringify(resposta)}`);
          assert.equal(resposta.valor, 25565, `${motor}: 1 · o MOD não recebeu o número da porta: ${JSON.stringify(resposta)}`);
          const chamada = await pagina.evaluate(() => entradasNaPonte().at(-1).args);
          assert.equal(chamada.id, "x/jogo", `${motor}: 1 · ${JSON.stringify(chamada)}`);
          assert.equal(chamada.nome, "minecraft-1", `${motor}: 1 · ${JSON.stringify(chamada)}`);
          assert.equal(chamada.geracao, 1, `${motor}: 1 · ${JSON.stringify(chamada)}`);
          assert.equal(Object.hasOwn(chamada, "gesto"), false, `${motor}: 1 · o bilhete foi para o Rust, e a fronteira é daqui`);
          const faixa = await pagina.evaluate(() => faixa());
          assert.match(faixa, /porta 25565 aberta para minecraft-1 \(x\/jogo\)/, `${motor}: 1 · ${faixa}`);
        });

        // 2. Sem bilhete: recusado pelo nome, e nada vai ao Rust.
        await caso(motor, "2", async () => {
          const antes = await entradas();
          const resposta = await entrar("minecraft-sem-clique", undefined);
          assert.equal(resposta?.ok, false, `${motor}: 2 · entrou sem um clique: ${JSON.stringify(resposta)}`);
          assert.equal(resposta.erro, "fora-de-um-clique", `${motor}: 2 · ${JSON.stringify(resposta)}`);
          assert.equal(await entradas(), antes, `${motor}: 2 · o invoke saiu sem um clique`);
          // Um bilhete inventado também não vale, e não gasta o de verdade.
          const gesto = await clicar();
          const inventado = await entrar("minecraft-inventado", "00000000000000000000000000000000");
          assert.equal(inventado.erro, "fora-de-um-clique", `${motor}: 2 · um bilhete inventado passou: ${JSON.stringify(inventado)}`);
          assert.equal(await entradas(), antes, `${motor}: 2 · o invoke saiu com um bilhete inventado`);
          const certo = await entrar("minecraft-2a", gesto);
          assert.equal(certo.ok, true, `${motor}: 2 · o bilhete certo foi gasto pelo inventado: ${JSON.stringify(certo)}`);
          // A recusa é do MOD e do registro, e **não da faixa**: sem clique, quem
          // joga não pediu nada, e um MOD não escreve na faixa à vontade.
          const faixa = await pagina.evaluate(() => faixa());
          assert.doesNotMatch(faixa, /minecraft-sem-clique|inventado|um clique seu/, `${motor}: 2 · a recusa sem clique foi para a faixa: ${faixa}`);
          const noRegistro = await pagina.evaluate(() => chamadas.filter((c) => c.cmd === "registrar_da_janela").map((c) => c.args.oQue).join("\n"));
          assert.match(noRegistro, /«tunel-entrar» falhou.*bilhete de um clique/, `${motor}: 2 · a recusa sem clique não foi ao registro`);
        });

        // 3. O mesmo bilhete duas vezes: a segunda é recusada.
        await caso(motor, "3", async () => {
          const gesto = await clicar();
          const antes = await entradas();
          const primeira = await entrar("minecraft-3a", gesto);
          assert.equal(primeira.ok, true, `${motor}: 3 · ${JSON.stringify(primeira)}`);
          const segunda = await entrar("minecraft-3b", gesto);
          assert.equal(segunda.ok, false, `${motor}: 3 · o bilhete valeu duas vezes: ${JSON.stringify(segunda)}`);
          assert.equal(segunda.erro, "fora-de-um-clique", `${motor}: 3 · ${JSON.stringify(segunda)}`);
          assert.equal(await entradas(), antes + 1, `${motor}: 3 · a segunda saiu para o Rust`);
        });

        // 4. Depois de 2 s o bilhete morreu; dentro dos 2 s ele vale.
        await caso(motor, "4", async () => {
          const gesto = await clicar();
          const antes = await entradas();
          await pagina.evaluate(() => { deslocamento = 2500; });
          const tarde = await entrar("minecraft-4a", gesto);
          assert.equal(tarde.ok, false, `${motor}: 4 · o bilhete valeu depois de 2 s: ${JSON.stringify(tarde)}`);
          assert.equal(tarde.erro, "fora-de-um-clique", `${motor}: 4 · ${JSON.stringify(tarde)}`);
          assert.equal(await entradas(), antes, `${motor}: 4 · o invoke saiu com o bilhete vencido`);
          await pagina.evaluate(() => { deslocamento = 0; });
          const outro = await clicar();
          await pagina.evaluate(() => { deslocamento = 1500; });
          const a_tempo = await entrar("minecraft-4b", outro);
          await pagina.evaluate(() => { deslocamento = 0; });
          assert.equal(a_tempo.ok, true, `${motor}: 4 · o bilhete de 1,5 s foi recusado: ${JSON.stringify(a_tempo)}`);
        });

        // 5. A porta do jogo estava ocupada: a faixa diz qual usar.
        await caso(motor, "5", async () => {
          await pagina.evaluate(() => { table.tunel_entrar = () => ({ porta: 25566, preferida: false, desejada: 25565 }); });
          const gesto = await clicar();
          const resposta = await entrar("minecraft-5", gesto);
          assert.equal(resposta.valor, 25566, `${motor}: 5 · ${JSON.stringify(resposta)}`);
          const faixa = await pagina.evaluate(() => faixa());
          assert.match(faixa, /a 25565 estava ocupada; use localhost:25566/, `${motor}: 5 · ${faixa}`);
          await pagina.evaluate(() => { table.tunel_entrar = () => ({ porta: 25565, preferida: true, desejada: 25565 }); });
        });

        // 8. O relatório de um túnel direto.
        await caso(motor, "8", async () => {
          await pagina.evaluate(() => emitir("seele://event", { TunnelReport: { report: {
            mod_id: "x/jogo", nome: "minecraft-1", porta: 25565, preferida: true, desejada: 25565,
            caminho: { tipo: "direto", par: 7, apelido: "Fulano" }, latencia_ms: 34,
            bytes_por_segundo: 122880, bytes_por_segundo_ida: 20480, bytes_por_segundo_volta: 102400,
            conexoes: 1, quedas: [], udp_descartados: 0, encerrado: null,
          } } }));
          const faixa = await pagina.evaluate(() => faixa());
          assert.match(
            faixa,
            /porta 25565 aberta para minecraft-1 \(x\/jogo\) · ligação direta com Fulano · 34 ms · 120 KB\/s/,
            `${motor}: 8 · ${faixa}`,
          );
        });

        // 9. A queda de um túnel pela estrela: o caminho e a frase da queda.
        await caso(motor, "9", async () => {
          await pagina.evaluate(() => emitir("seele://event", { TunnelReport: { report: {
            mod_id: "x/jogo", nome: "minecraft-5", porta: 25566, preferida: false, desejada: 25565,
            caminho: { tipo: "estrela" }, latencia_ms: null,
            bytes_por_segundo: 0, bytes_por_segundo_ida: 0, bytes_por_segundo_volta: 0,
            conexoes: 0, quedas: [{ motivo: "caminho-caiu", vezes: 2 }, { motivo: "fechado", vezes: 5 }],
            udp_descartados: 0, encerrado: null,
          } } }));
          const faixa = await pagina.evaluate(() => faixa());
          const linha = faixa.split("\n").join(" ");
          assert.match(linha, /minecraft-5 \(x\/jogo\) · pela estrela/, `${motor}: 9 · ${faixa}`);
          assert.match(linha, /última queda: a conexão com o jogo caiu \(2 vezes\)/, `${motor}: 9 · ${faixa}`);
          assert.doesNotMatch(linha, /rede/, `${motor}: 9 · a queda acusa a rede, e ela também é a do jogo que recusou: ${faixa}`);
          assert.doesNotMatch(linha, /conexão foi fechada/, `${motor}: 9 · uma conexão que acabou como devia virou queda: ${faixa}`);
        });

        // 10. O túnel que acabou: o motivo em frase e o DISPENSAR.
        await caso(motor, "10", async () => {
          await pagina.evaluate(() => emitir("seele://event", { TunnelReport: { report: {
            mod_id: "x/jogo", nome: "minecraft-3a", porta: 25565, preferida: true, desejada: 25565,
            caminho: { tipo: "estrela" }, latencia_ms: null,
            bytes_por_segundo: 0, bytes_por_segundo_ida: 0, bytes_por_segundo_volta: 0,
            conexoes: 0, quedas: [], udp_descartados: 0, encerrado: "maquina-saiu",
          } } }));
          const faixa = await pagina.evaluate(() => faixa());
          assert.match(faixa.split("\n").join(" "), /minecraft-3a \(x\/jogo\) fechou: a máquina do jogo saiu do servidor/, `${motor}: 10 · ${faixa}`);
          // Quem fecha é quem joga (ou o MOD ao sair): «fechada», e não a frase
          // de uma conexão que acabou.
          await pagina.evaluate(() => emitir("seele://event", { TunnelReport: { report: {
            mod_id: "x/jogo", nome: "minecraft-4a", porta: 25565, preferida: true, desejada: 25565,
            caminho: { tipo: "estrela" }, latencia_ms: null,
            bytes_por_segundo: 0, bytes_por_segundo_ida: 0, bytes_por_segundo_volta: 0,
            conexoes: 0, quedas: [], udp_descartados: 0, encerrado: "fechado",
          } } }));
          assert.match((await pagina.evaluate(() => faixa())).split("\n").join(" "), /minecraft-4a \(x\/jogo\) fechou: a porta foi fechada/, `${motor}: 10 · o fim por quem abriu`);
          await pagina.click('#tuneis [data-nome="minecraft-3a"] .tunel-dispensar');
          const depois = await pagina.evaluate(() => faixa());
          assert.doesNotMatch(depois, /minecraft-3a/, `${motor}: 10 · o DISPENSAR não tirou a linha: ${depois}`);
        });

        // 11. A recusa do servidor chega ao MOD pelo nome e à faixa em frase.
        await caso(motor, "11", async () => {
          await pagina.evaluate(() => { table.tunel_entrar = () => { throw { Recusado: { motivo: "sem-permissao" } }; }; });
          const gesto = await clicar();
          const resposta = await entrar("minecraft-11", gesto);
          assert.equal(resposta.ok, false, `${motor}: 11 · ${JSON.stringify(resposta)}`);
          assert.equal(resposta.erro, "sem-permissao", `${motor}: 11 · o MOD não recebeu o nome da recusa: ${JSON.stringify(resposta)}`);
          const faixa = await pagina.evaluate(() => faixa());
          assert.match(
            faixa.split("\n").join(" "),
            /x\/jogo não abriu minecraft-11: você não tem a permissão de entrar no jogo/,
            `${motor}: 11 · ${faixa}`,
          );
          await pagina.evaluate(() => { table.tunel_entrar = () => ({ porta: 25565, preferida: true, desejada: 25565 }); });
        });

        // 12. A faixa é do produto: fora de toda superfície e região de MOD.
        await caso(motor, "12", async () => {
          const dentro = await pagina.evaluate(() => {
            const faixa = $("tuneis");
            return {
              emSuperficie: Boolean(faixa.closest("[data-superficie], [data-chave-do-mod], .superficie-de-mod, .folha-de-mod")),
              filhosDeMod: faixa.querySelectorAll("[data-chave-do-mod]").length,
            };
          });
          assert.deepEqual(dentro, { emSuperficie: false, filhosDeMod: 0 }, `${motor}: 12 · ${JSON.stringify(dentro)}`);
        });

        // 14. O nome do MOD não escreve na faixa à vontade: fora de
        //     `[a-z0-9-]{1,32}` é recusado na casca (`nao-existe`), sem invoke e
        //     sem linha; e a recusa sem clique, repetida, não acrescenta nada.
        await caso(motor, "14", async () => {
          const antes = await entradas();
          const blocos = () => pagina.evaluate(() => $("tuneis").querySelectorAll(".tunel-bloco").length);
          const nBlocos = await blocos();
          for (const nome of ["Maiuscula", "com espaço", "<img src=x onerror=1>", "-borda", "a".repeat(33), "", 7, null, { x: 1 }]) {
            const gesto = await clicar();
            const r = await pagina.evaluate(([nome, gesto]) => pedir(jogo, { tipo: "tunel-entrar", nome, gesto }), [nome, gesto]);
            assert.equal(r?.erro, "nao-existe", `${motor}: 14 · o nome ${JSON.stringify(nome)} não foi recusado pelo nome: ${JSON.stringify(r)}`);
          }
          for (let i = 0; i < 25; i += 1) await entrar(`sem-clique-${i}`, undefined);
          assert.equal(await entradas(), antes, `${motor}: 14 · um nome inválido ou um pedido sem clique chegou ao Rust`);
          assert.equal(await blocos(), nBlocos, `${motor}: 14 · a faixa cresceu com o que o MOD pediu sem clique ou com nome inválido`);
          assert.doesNotMatch(await pagina.evaluate(() => faixa()), /Maiuscula|onerror|sem-clique-/, `${motor}: 14 · texto do MOD na faixa`);
        });

        // 15. Uma linha de recusa por MOD, trocada e não somada.
        await caso(motor, "15", async () => {
          await pagina.evaluate(() => { table.tunel_entrar = () => { throw { Recusado: { motivo: "nome-tomado" } }; }; });
          for (const nome of ["jogo-a", "jogo-b", "jogo-c"]) {
            const gesto = await clicar();
            const r = await entrar(nome, gesto);
            assert.equal(r.erro, "nome-tomado", `${motor}: 15 · ${JSON.stringify(r)}`);
          }
          await pagina.evaluate(() => { table.tunel_entrar = () => ({ porta: 25565, preferida: true, desejada: 25565 }); });
          const recusadas = await pagina.evaluate(() => [...$("tuneis").querySelectorAll('[data-estado="recusada"]')].map((b) => b.dataset.nome));
          assert.deepEqual(recusadas, ["jogo-c"], `${motor}: 15 · ${JSON.stringify(recusadas)}`);
        });

        // 16. O relatório que chega antes de o `invoke` voltar não se perde.
        await caso(motor, "16", async () => {
          await pagina.evaluate(() => emitir("seele://event", { TunnelReport: { report: {
            mod_id: "x/jogo", nome: "mescla", porta: 25565, preferida: true, desejada: 25565,
            caminho: { tipo: "direto", par: 7, apelido: "Fulano" }, latencia_ms: 12,
            bytes_por_segundo: 0, bytes_por_segundo_ida: 0, bytes_por_segundo_volta: 0,
            conexoes: 0, quedas: [], udp_descartados: 0, encerrado: null,
          } } }));
          await pagina.evaluate(() => window.dispatchEvent(new CustomEvent("seele-tunel-aberto", { detail: { modId: "x/jogo", nome: "mescla", porta: 25565, preferida: true, desejada: 25565 } })));
          const linha = (await pagina.evaluate(() => faixa())).split("\n").join(" ");
          assert.match(linha, /mescla \(x\/jogo\) · ligação direta com Fulano · 12 ms/, `${motor}: 16 · ${linha}`);
        });

        // 7. Um MOD sem `porta` que posta `tunel-entrar` direto.
        await caso(motor, "7", async () => {
          const antes = await entradas();
          const resposta = await pagina.evaluate(() => pedir(semPorta, { tipo: "tunel-entrar", nome: "x", gesto: "00000000000000000000000000000000" }));
          assert.equal(resposta?.ok, false, `${motor}: 7 · ${JSON.stringify(resposta)}`);
          assert.equal(resposta.erro, "tipo-desconhecido", `${motor}: 7 · ${JSON.stringify(resposta)}`);
          assert.equal(await entradas(), antes, `${motor}: 7 · o invoke saiu para um MOD sem a porta`);
        });

        // 6. Desligar o MOD de jogo: `tunel_sair_do_mod` com o id, e a linha some.
        await caso(motor, "6", async () => {
          assert.match(await pagina.evaluate(() => faixa()), /\(x\/jogo\)/, `${motor}: 6 · a faixa não tinha o MOD antes de desligá-lo`);
          await pagina.evaluate(async () => {
            table.mods_instalados = [];
            pedirAoServidor = async () => ({ ok: true, conjunto: "conj-1", mods: [] });
            await carregarMods();
          });
          await pagina.waitForFunction(() => chamadas.some((c) => c.cmd === "tunel_sair_do_mod"), null, { timeout: 5000 })
            .catch(() => { throw new Error(`${motor}: 6 · o MOD saiu e tunel_sair_do_mod não foi chamado`); });
          const saida = await pagina.evaluate(() => chamadas.filter((c) => c.cmd === "tunel_sair_do_mod").map((c) => c.args));
          assert.deepEqual(
            saida.map(({ id, carimbo }) => ({ id, carimbo })),
            [{ id: "x/jogo", carimbo: { sessao: 1, geracao: 1 } }],
            `${motor}: 6 · ${JSON.stringify(saida)}`,
          );
          await pagina.waitForFunction(() => !/\(x\/jogo\)/.test(faixa()), null, { timeout: 5000 })
            .catch(async () => { throw new Error(`${motor}: 6 · a linha do MOD não saiu da faixa: ${await pagina.evaluate(() => faixa())}`); });
          // O que o MOD sem a porta não fecha: nada foi pedido por ele.
          assert.equal(saida.some(({ id }) => id === "x/sem-porta"), false, `${motor}: 6 · fechou túneis de um MOD sem porta`);
        });

        // 13. A troca de vista: o MOD da sessão 1 sai com a vista já na 2, e o
        //     fecho leva o carimbo da sessão DELE — a porta fecha lá, e a casca
        //     não diz que a sessão acabou.
        await caso(motor, "13", async () => {
          await pagina.evaluate(() => {
            window.outro = (() => {
              const mod = { id: "x/outro", hash: "h-outro", api: 8, poderes: ["porta"] };
              const instancia = new InstanciaDeMod(mod.id, mod.hash, geracaoDaSessao, {
                pedirEncerramento() {}, async encerrou() { return true; }, entregar() {},
              });
              instancia.estado = ESTADOS_DE_MOD.ativa;
              modsCarregados.set(mod.id, instancia);
              ligarOFechoDosTuneis(mod, instancia);
              return instancia;
            })();
            chamadas.length = 0;
            porNaVista({ sessao: 2, geracao: 1 });
            outro.encerrar();
          });
          await pagina.waitForFunction(() => chamadas.some((c) => c.cmd === "tunel_sair_do_mod"), null, { timeout: 5000 })
            .catch(() => { throw new Error(`${motor}: 13 · o MOD saiu e tunel_sair_do_mod não foi chamado`); });
          const chamada = await pagina.evaluate(() => chamadas.find((c) => c.cmd === "tunel_sair_do_mod").args);
          await pagina.evaluate(() => porNaVista({ sessao: 1, geracao: 1 }));
          assert.deepEqual(chamada.carimbo, { sessao: 1, geracao: 1 }, `${motor}: 13 · o fecho foi para a sessão da vista, e não para a do MOD: ${JSON.stringify(chamada)}`);
          const log = await pagina.evaluate(() => chamadas.filter((c) => c.cmd === "registrar_da_janela").map((c) => c.args.oQue).join("\n"));
          assert.doesNotMatch(log, /x\/outro.*sessao-encerrada/, `${motor}: 13 · a sessão viva foi dita como encerrada: ${log}`);
        });

        await caso(motor, "página", async () => assert.deepEqual(erros, [], `${motor}: erros na página`));
        console.log(JSON.stringify({ motor, casos: 16, falhas: falhas.filter((f) => f.startsWith(motor)).length }));
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
