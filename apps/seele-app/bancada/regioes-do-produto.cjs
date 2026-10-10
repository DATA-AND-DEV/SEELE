// As regiões da API 6, no `index.html` real e no Chromium (rascunho da API 6,
// §3; tarefa 5 do plano da F3).
//
// # Por que esta bancada existe
//
// Os guardas de `frontend.rs` conferem que cada região tem nome, contêiner e
// alguém que pergunta por ela. Não conferem que o nó de um MOD aparece onde a
// região diz, nem que a apresentação nativa volta quando ele sai — e é isso
// que um MOD que registra e não vê nada encontraria. A bancada de
// contribuições roda num `vm`, sem navegador («não é navegador»); esta mede a
// página.
//
// # O que ela mede, por região
//
// 1. registrar `adicionar`: o nó do MOD aparece **dentro** do contêiner da
//    região;
// 2. o que a região deixa ao produto continua lá, intacto (o servidor atual e
//    «conectar a outro», na trilha; o SAIR e o microfone, na barra do
//    operador; …);
// 3. revogar: o nó sai, e a região volta a ser o que era antes, do estado
//    atual.
//
// Rodar:  node apps/seele-app/bancada/regioes-do-produto.cjs

const path = require("node:path");

const root = path.resolve(__dirname, "../../..");
const { servir, respostas, retrato } = require(path.join(root, "apps/seele-app/bancada/telas.cjs"));
const chromium = require("./playwright.cjs").chromium();

/**
 * As regiões medidas: o ponto, o seletor do contêiner, e o que o produto
 * mantém nela — seletores que precisam continuar na página, do mesmo jeito,
 * com o MOD de pé.
 */
const REGIOES = [
  {
    ponto: "redes",
    conteiner: "#trilha-mods",
    doProduto: ["#trilha-server", "#trilha-adicionar"],
    teto: { aVista: ["#trilha-server", "#trilha-adicionar"], descobertos: ["#trilha-server", "#trilha-adicionar"] },
  },
  {
    ponto: "operador.barra",
    conteiner: "#operador-mods",
    // O núcleo (§10): o microfone, o fone, o «no ar» e o SAIR.
    doProduto: ["#botao-mudo", "#botao-surdo", "#voz-estado", "#operador-sair"],
    // Conteúdo maior que a região rola por dentro, e o núcleo continua inteiro
    // na janela.
    teto: { aVista: ["#botao-mudo", "#operador-sair"], descobertos: ["#botao-mudo", "#botao-surdo", "#operador-sair"] },
  },
  {
    ponto: "compositor",
    conteiner: "#compositor-mods",
    // A caixa de escrever, o anexo e o ENVIAR: quem envia é a pessoa.
    doProduto: ["#campo-mensagem", "#botao-anexar", "#botao-enviar"],
    teto: { aVista: ["#campo-mensagem", "#botao-enviar"], descobertos: ["#campo-mensagem", "#botao-enviar"] },
  },
  {
    ponto: "status.barra",
    conteiner: "#telemetria-mods",
    // Atraso, jitter, distúrbio e codec: o que a pessoa lê para saber se a voz
    // está chegando.
    doProduto: ["#tel-rtt", "#tel-jit", "#tel-loss", "#tel-codec"],
    // E o aviso de falha local, que é do produto e fica depois da região:
    // escondido no retrato da bancada, ele é mostrado para a medida. Sem o
    // `overflow: hidden` da região, ele saía da janela (1 302–1 387 px numa
    // janela de 1 280, medido).
    teto: {
      aVista: ["#tel-rtt", "#tel-codec", "#tel-local"],
      mostrar: ["#tel-local"],
      descobertos: ["#tel-rtt", "#tel-codec", "#tel-local"],
    },
  },
  {
    // O que um MOD acrescenta à lista de pessoas, acima dela; a contagem e as
    // seções das salas são do produto.
    ponto: "pessoas.lista",
    conteiner: "#roster-mods",
    doProduto: ["#pessoas-conta", "#lista-roster .roster-sala"],
    teto: {
      aVista: ["#pessoas-conta", "#lista-roster .roster-sala"],
      descobertos: ["#pessoas-conta", "#lista-roster .roster-sala"],
    },
  },
  {
    // **Substituir é desenhar o miolo** (rascunho §3): entre o nome do canal e
    // a lupa, que são do produto — o nome é também o botão de renomear.
    ponto: "canal.cabecalho",
    modo: "substituir",
    alvoDoCanalAberto: true,
    conteiner: "#canal-cabecalho-miolo",
    doProduto: ["#linha-nome", "#botao-buscar"],
    // E a conversa embaixo: sem teto, um miolo alto fazia a barra crescer até
    // 1 149 px, e a lista de mensagens ia para y = 1 230, fora da janela
    // (medido).
    teto: {
      aVista: ["#linha-nome", "#botao-buscar", "#lista-mensagens"],
      descobertos: ["#linha-nome", "#botao-buscar"],
    },
  },
];

(async () => {
  const server = servir();
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ headless: true });
  const falhas = [];
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    page.on("pageerror", (erro) => falhas.push(`erro na página: ${erro.message}`));
    await page.addInitScript(
      (table) => {
        window.testTable = table;
        window.__TAURI__ = {
          core: { invoke: async (cmd) => (Object.hasOwn(table, cmd) ? table[cmd] : null) },
          event: { listen: async () => () => {} },
          window: {
            getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }),
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
    await page.waitForSelector(".mensagens .corpo");

    for (const regiao of REGIOES) {
      // Uma região que lança não esconde as outras: a falha é dela, e a rodada
      // segue.
      const medido = await page.evaluate(async ({ ponto, conteiner, doProduto, teto, modo = "adicionar", alvoDoCanalAberto }) => {
        const r = window.testTable.snapshot;
        const alvo = alvoDoCanalAberto ? String(r.channels.find((c) => c.open).id) : undefined;
        const quadro = () => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
        const doProdutoAgora = () => doProduto.map((seletor) => document.querySelector(seletor)?.outerHTML ?? null);
        // O tamanho, e não só a marcação: um MOD largo espremeria as métricas
        // da barra de status sem mudar um caractere delas.
        const tamanhos = () => doProduto.map((seletor) => {
          const caixa = document.querySelector(seletor)?.getBoundingClientRect();
          return caixa ? `${Math.round(caixa.width)}×${Math.round(caixa.height)}` : null;
        });
        const tamanhosAntes = tamanhos();
        const caixa = document.querySelector(conteiner);
        const antes = { conteiner: caixa?.innerHTML ?? null, produto: doProdutoAgora() };

        // Uma instância viva, como a de um MOD carregado: sem ela a contribuição
        // não monta (`montarContribuicao` pede `instancia.admite`).
        // O executor guarda o que é entregue ao MOD. Sem `entregar`, o aviso de
        // «não coube» lançava aqui e a montagem inteira virava «não montou» —
        // um artefato da bancada que já foi lido como comportamento do produto.
        const instancia = new InstanciaDeMod("mod/regiao", "h", geracaoDaSessao, {
          pedirEncerramento() {},
          async encerrou() { return true; },
          entregar() {},
        });
        instancia.estado = ESTADOS_DE_MOD.ativa;
        modsCarregados.set("mod/regiao", instancia);
        const { handle } = contribuicoesDosMods.registrar({ id: "mod/regiao" }, instancia, {
          ponto,
          modo,
          alvo,
          conteudo: [{ forma: "texto", chave: "marca", dentro: `marca de ${ponto}` }],
        });
        desenhar(r);
        await quadro();
        const dentro = [...(document.querySelector(conteiner)?.querySelectorAll("*") ?? [])]
          .some((no) => no.textContent === `marca de ${ponto}` && no.children.length === 0);
        const emOutroLugar = [...document.querySelectorAll("body *")]
          .some((no) => no.textContent === `marca de ${ponto}` && no.children.length === 0
            && !no.closest(conteiner));
        const comOMod = doProdutoAgora();
        const tamanhosComOMod = tamanhos();

        contribuicoesDosMods.revogar(handle, { id: "mod/regiao", instancia });
        modsCarregados.delete("mod/regiao");
        desenhar(r);
        await quadro();
        const depois = { conteiner: document.querySelector(conteiner)?.innerHTML ?? null, produto: doProdutoAgora() };

        // **O teto**: 16 linhas longas, ~1 700 px de conteúdo — mais do que a
        // janela de 860 px, e dentro dos 64 nós de um cartão. Medido: com o
        // teto, a região mostra 96 px e o SAIR fica a 778 px do topo; sem ele,
        // o SAIR vai a 1 906 px, fora da janela. Muitas linhas curtas não
        // servem: 30 (480 px) cabem na janela, encolhendo a lista de canais
        // acima, e acima de 32 o orçamento do cartão corta o resto.
        //
        // (Uma versão anterior deste comentário dizia que 60 linhas «não
        // montavam nada». Era a bancada: o executor falso não tinha `entregar`,
        // o aviso de «não coube» lançava, e a montagem virava «não montou». O
        // produto corta e avisa — ver «o orçamento de cada ponto», abaixo.)
        let foraDaVista = null;
        let tamanhosNoTeto = null;
        let cobertos = null;
        if (teto) {
          // **O desenho deslocado, mirando**: `estilo.mover` vai a ±512 px
          // (`mods-estilos.js`), e é assim que um MOD poria o desenho dele por
          // cima do que está ao lado da região. Os nós deslocados vêm primeiro
          // — a base deles é o canto da região — e cada um vai do canto até o
          // centro de um item do produto. Um deslocamento fixo de ±512 caía ao
          // lado dos alvos e passava sem a região conter nada (medido).
          // O que a bancada mostra para medir é mostrado **depois** de cada
          // desenho: o desenho do produto o esconde de novo.
          const mostrar = () => { for (const seletor of teto.mostrar ?? []) document.querySelector(seletor).hidden = false; };
          const linhas = [
            ...Array.from({ length: 16 }, (_, i) => ({ forma: "texto", chave: `l${i}`, dentro: `linha ${i} `.repeat(24) })),
            // E uma linha sem quebra, ~2 500 px: é ela que espremeria uma
            // barra de altura fixa na horizontal.
            { forma: "texto", chave: "largo", dentro: "LARGO".repeat(80) },
          ];
          const semMira = contribuicoesDosMods.registrar({ id: "mod/regiao" }, instancia, { ponto, modo, alvo, conteudo: linhas });
          desenhar(r);
          mostrar();
          await quadro();
          const canto = document.querySelector(conteiner).getBoundingClientRect();
          const limite = (n) => Math.max(-512, Math.min(512, Math.round(n)));
          const mirados = (teto.descobertos ?? []).map((seletor, i) => {
            const centro = document.querySelector(seletor).getBoundingClientRect();
            return {
              forma: "texto", chave: `mirado${i}`, dentro: "█".repeat(6),
              estilo: {
                mover: {
                  x: limite(centro.left + centro.width / 2 - canto.left - 8),
                  y: limite(centro.top + centro.height / 2 - canto.top - 8),
                },
              },
            };
          });
          contribuicoesDosMods.revogar(semMira.handle, { id: "mod/regiao", instancia });
          const grande = contribuicoesDosMods.registrar({ id: "mod/regiao" }, instancia, {
            ponto, modo, alvo, conteudo: [...mirados, ...linhas],
          });
          modsCarregados.set("mod/regiao", instancia);
          desenhar(r);
          mostrar();
          await quadro();
          tamanhosNoTeto = tamanhos();
          foraDaVista = teto.aVista.filter((seletor) => {
            const caixaDoNucleo = document.querySelector(seletor)?.getBoundingClientRect();
            return !caixaDoNucleo || caixaDoNucleo.height === 0
              || caixaDoNucleo.top < 0 || caixaDoNucleo.bottom > innerHeight
              || caixaDoNucleo.left < 0 || caixaDoNucleo.right > innerWidth;
          });
          // **Nada do MOD por cima do produto**: no centro de cada um, o elemento
          // que recebe o clique é ele, ou um filho dele.
          cobertos = (teto.descobertos ?? []).filter((seletor) => {
            const alvoDoProduto = document.querySelector(seletor);
            const caixaDoAlvo = alvoDoProduto?.getBoundingClientRect();
            if (!caixaDoAlvo) return true;
            const noCentro = document.elementFromPoint(
              caixaDoAlvo.left + caixaDoAlvo.width / 2, caixaDoAlvo.top + caixaDoAlvo.height / 2,
            );
            return !noCentro || !alvoDoProduto.contains(noCentro);
          });
          for (const seletor of teto.mostrar ?? []) document.querySelector(seletor).hidden = true;
          contribuicoesDosMods.revogar(grande.handle, { id: "mod/regiao", instancia });
          modsCarregados.delete("mod/regiao");
          desenhar(r);
          await quadro();
        }
        return {
          existe: Boolean(caixa), antes, dentro, emOutroLugar, comOMod, depois, foraDaVista,
          tamanhosAntes, tamanhosComOMod, tamanhosNoTeto, cobertos,
        };
      }, regiao).catch((erro) => ({ erro: erro.message.split("\n")[0] }));

      const { ponto } = regiao;
      if (medido.erro) {
        falhas.push(`${ponto}: ${medido.erro}`);
        continue;
      }
      if (!medido.existe) {
        falhas.push(`${ponto}: o contêiner ${regiao.conteiner} não está na página`);
        continue;
      }
      if (!medido.dentro) falhas.push(`${ponto}: registrado, e o nó do MOD não apareceu em ${regiao.conteiner}`);
      if (medido.emOutroLugar) falhas.push(`${ponto}: o nó do MOD apareceu fora de ${regiao.conteiner}`);
      regiao.doProduto.forEach((seletor, i) => {
        if (medido.antes.produto[i] === null) falhas.push(`${ponto}: ${seletor}, do produto, não está na página`);
        else if (medido.comOMod[i] !== medido.antes.produto[i]) {
          falhas.push(`${ponto}: ${seletor}, do produto, mudou com o MOD de pé`);
        } else if (medido.tamanhosComOMod[i] !== medido.tamanhosAntes[i]) {
          falhas.push(`${ponto}: ${seletor}, do produto, mudou de tamanho com o MOD de pé `
            + `(${medido.tamanhosAntes[i]} → ${medido.tamanhosComOMod[i]})`);
        }
      });
      if (medido.depois.conteiner !== medido.antes.conteiner) {
        falhas.push(`${ponto}: revogado, e ${regiao.conteiner} não voltou ao que era`);
      }
      if (medido.depois.produto.some((html, i) => html !== medido.antes.produto[i])) {
        falhas.push(`${ponto}: revogado, e o que é do produto não voltou ao que era`);
      }
      medido.tamanhosNoTeto?.forEach((tamanho, i) => {
        if (tamanho !== medido.tamanhosAntes[i]) {
          falhas.push(`${ponto}: com conteúdo grande, ${regiao.doProduto[i]}, do produto, mudou de tamanho `
            + `(${medido.tamanhosAntes[i]} → ${tamanho})`);
        }
      });
      if (medido.cobertos?.length) {
        falhas.push(`${ponto}: com conteúdo grande, o MOD ficou por cima de ${medido.cobertos.join(" e ")}`);
      }
      if (medido.foraDaVista?.length) {
        const quais = medido.foraDaVista;
        falhas.push(`${ponto}: com conteúdo maior que a região, ${quais.join(" e ")} `
          + `${quais.length > 1 ? "saíram" : "saiu"} da vista`);
      }
    }

    // ------------------------------------- a linha de uma pessoa, apresentada
    //
    // `pessoa.cartao` substituído (rascunho §3 e §10; D-nucleo-1): a linha é
    // `[núcleo] [miolo] [⋯]`. O apelido verificado fica no núcleo, **à vista**,
    // lido do retrato pela chave da linha, e é o botão de moderação; o cartão do
    // MOD fica no miolo, numa caixa fechada. Até a tarefa 5 o apelido e a
    // moderação moravam dentro do «⋯» recolhido.
    const linha = await page.evaluate(async () => {
      const r = window.testTable.snapshot;
      const quadro = () => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const lia = r.voice_rooms.flatMap((sala) => sala.people).find((p) => !p.is_self);
      const instancia = new InstanciaDeMod("mod/linha", "h", geracaoDaSessao, {
        pedirEncerramento() {}, async encerrou() { return true; }, entregar() {},
      });
      instancia.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set("mod/linha", instancia);
      const registrar = (conteudo) => contribuicoesDosMods.registrar({ id: "mod/linha" }, instancia, {
        ponto: "pessoa.cartao", modo: "substituir", alvo: String(lia.id), conteudo,
      }).handle;
      // O cartão escreve outro nome: o núcleo continua dizendo quem é.
      const cartao = [{ forma: "texto", chave: "nome", dentro: "alice" }];
      let handle = registrar(cartao);
      desenhar(r);
      await quadro();
      const achar = () => document.querySelector('li.pessoa-apresentada[data-apresentada-por="mod/linha"]');
      const apelidoDe = (li) => li?.querySelector(".pessoa-nucleo .pessoa-nome");
      // A mira: do canto do miolo até o centro do apelido.
      const miolo = achar()?.querySelector(".pessoa-miolo")?.getBoundingClientRect();
      const alvo = apelidoDe(achar())?.getBoundingClientRect();
      if (miolo && alvo) {
        contribuicoesDosMods.revogar(handle, { id: "mod/linha", instancia });
        const limite = (n) => Math.max(-512, Math.min(512, Math.round(n)));
        handle = registrar([
          {
            forma: "texto", chave: "mirado", dentro: "█".repeat(8),
            estilo: { mover: { x: limite(alvo.left + alvo.width / 2 - miolo.left - 8), y: limite(alvo.top + alvo.height / 2 - miolo.top - 8) } },
          },
          ...cartao,
        ]);
        desenhar(r);
        await quadro();
      }
      const li = achar();
      const apelido = apelidoDe(li);
      const caixa = apelido?.getBoundingClientRect();
      const noCentro = caixa ? document.elementFromPoint(caixa.left + caixa.width / 2, caixa.top + caixa.height / 2) : null;
      const moderar = li?.querySelector(`.pessoa-nucleo [data-moderar-pessoa="${lia.id}"]`);
      const medido = {
        achou: Boolean(li),
        apelido: apelido?.textContent ?? null,
        esperado: lia.nickname,
        escondido: Boolean(apelido?.closest("details:not([open])")),
        naVista: Boolean(caixa && caixa.width > 0 && caixa.height > 0 && caixa.top >= 0 && caixa.bottom <= innerHeight),
        descoberto: Boolean(noCentro && apelido?.contains(noCentro)),
        moderar: Boolean(moderar && !moderar.closest("details:not([open])")),
        cartaoNoMiolo: Boolean(li?.querySelector(".pessoa-miolo")?.textContent.includes("alice")),
      };
      contribuicoesDosMods.revogar(handle, { id: "mod/linha", instancia });
      modsCarregados.delete("mod/linha");
      desenhar(r);
      await quadro();
      return medido;
    }).catch((erro) => ({ erro: erro.message.split("\n")[0] }));
    if (linha.erro) falhas.push(`pessoa.cartao: ${linha.erro}`);
    else if (!linha.achou) falhas.push("pessoa.cartao: a linha apresentada não está no roster");
    else {
      if (linha.apelido !== linha.esperado) {
        falhas.push(`pessoa.cartao: o núcleo diz «${linha.apelido}», e o retrato diz «${linha.esperado}»`);
      }
      if (linha.escondido || !linha.naVista) falhas.push("pessoa.cartao: o apelido verificado não está à vista");
      if (!linha.descoberto) falhas.push("pessoa.cartao: o desenho do MOD ficou por cima do apelido verificado");
      if (!linha.moderar) falhas.push("pessoa.cartao: a moderação não está a um clique, fora do «⋯»");
      if (!linha.cartaoNoMiolo) falhas.push("pessoa.cartao: o cartão do MOD não está no miolo");
    }

    // ------------------------------------- a lista de pessoas, substituída
    //
    // `pessoas.lista` substituída (rascunho §3; decisão (a) da orquestradora,
    // 09/10): **as seções das salas são do produto** — quem está na sala de
    // quem ouve você é verdade de confiança —, e os grupos do MOD valem dentro
    // de cada uma. Um grupo com Lia (na sala) e Rafa (fora das salas) deixa
    // cada um na sua seção; quem o MOD não agrupou fica sob OUTRAS PESSOAS.
    const lista = await page.evaluate(async () => {
      const r = window.testTable.snapshot;
      const quadro = () => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const secoes = () => [...$("lista-roster").querySelectorAll(":scope > li.roster-grupo")].map((grupo) => ({
        sala: grupo.querySelector(".roster-sala-nome")?.textContent ?? "",
        linhas: [...grupo.querySelector(".roster-pessoas").children].map((li) =>
          li.classList.contains("roster-subgrupo") ? `[${li.textContent}]` : (li.querySelector(".pessoa-nome")?.textContent ?? "?")),
      }));
      const antes = $("lista-roster").innerHTML;
      const instancia = new InstanciaDeMod("mod/lista", "h", geracaoDaSessao, {
        pedirEncerramento() {}, async encerrou() { return true; }, entregar() {},
      });
      instancia.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set("mod/lista", instancia);
      let recusa = "";
      try {
        contribuicoesDosMods.registrar({ id: "mod/lista" }, instancia, {
          ponto: "pessoas.lista", modo: "substituir", conteudo: [{ forma: "texto", dentro: "x" }],
        });
      } catch (erro) {
        recusa = erro.message;
      }
      const { handle } = contribuicoesDosMods.registrar({ id: "mod/lista" }, instancia, {
        ponto: "pessoas.lista", modo: "substituir",
        conteudo: { grupos: [{ titulo: "MESA 1", ids: [2, 3, 99] }] },
      });
      desenhar(r);
      await quadro();
      const com = secoes();
      contribuicoesDosMods.revogar(handle, { id: "mod/lista", instancia });
      modsCarregados.delete("mod/lista");
      desenhar(r);
      await quadro();
      return { recusa, com, voltou: $("lista-roster").innerHTML === antes };
    }).catch((erro) => ({ erro: erro.message.split("\n")[0] }));
    if (lista.erro) falhas.push(`pessoas.lista: ${lista.erro}`);
    else {
      if (!lista.recusa.includes("pessoas.lista")) {
        falhas.push(`pessoas.lista: uma substituição com outra forma não foi recusada pelo nome (${lista.recusa || "aceita"})`);
      }
      const esperado = [
        { sala: "Sala principal", linhas: ["[MESA 1]", "Lia", "[OUTRAS PESSOAS]", "Alex (você)"] },
        { sala: "NO SERVIDOR, FORA DAS SALAS", linhas: ["[MESA 1]", "Rafa"] },
      ];
      if (JSON.stringify(lista.com) !== JSON.stringify(esperado)) {
        falhas.push(`pessoas.lista: as seções saíram ${JSON.stringify(lista.com)}, e o esperado é ${JSON.stringify(esperado)}`);
      }
      if (!lista.voltou) falhas.push("pessoas.lista: revogada, e a lista não voltou ao que era");
    }

    // ------------------------------------- a mensagem, substituída
    //
    // `conversa.mensagem` (rascunho §3, aprovado em 09/10): substituída, o MOD
    // desenha **o corpo**; a cabeça (o apelido verificado, a hora, o REMOVER),
    // os acréscimos, o anexo e o selo do envio são do produto. A busca vence:
    // marcada, a mensagem volta ao corpo nativo, com o realce. E o agrupamento
    // é pela **chave** do autor: duas pessoas com o mesmo apelido, em seguida,
    // têm cada uma a sua cabeça.
    const mensagem = await page.evaluate(async () => {
      const r = window.testTable.snapshot;
      const agora = Math.floor(Date.now() / 1000);
      const deAntes = mensagens;
      mensagens = [
        { id: 11, channel: 1, author: 2, author_nickname: "Lia", at_seconds: agora - 60, body: "primeira da Lia", own: false, edited: false, attachment: null },
        { id: 12, channel: 1, author: 3, author_nickname: "Lia", at_seconds: agora - 50, body: "de outra Lia", own: false, edited: false, attachment: null },
      ];
      const item = (i) => document.querySelector(`#lista-mensagens [data-mensagem="${i}"]`);
      desenharMensagens();
      const cabecas = [0, 1].map((i) => item(i)?.querySelector(".mensagem-autor")?.textContent ?? null);
      const instancia = new InstanciaDeMod("mod/mensagem", "h", geracaoDaSessao, {
        pedirEncerramento() {}, async encerrou() { return true; }, entregar() {},
      });
      instancia.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set("mod/mensagem", instancia);
      const dono = { id: "mod/mensagem", instancia };
      const substituida = contribuicoesDosMods.registrar(dono, instancia, {
        ponto: "conversa.mensagem", modo: "substituir", alvo: "11",
        conteudo: [{ forma: "texto", chave: "c", dentro: "corpo do MOD" }],
      }).handle;
      const acrescida = contribuicoesDosMods.registrar(dono, instancia, {
        ponto: "conversa.mensagem", modo: "adicionar", alvo: "11",
        conteudo: [{ forma: "texto", chave: "a", dentro: "acréscimo" }],
      }).handle;
      desenharMensagens();
      const comOMod = {
        miolo: item(0)?.querySelector(".mensagem-miolo")?.textContent ?? null,
        corpoNativo: Boolean(item(0)?.querySelector(".corpo")),
        autor: item(0)?.querySelector(".mensagem-autor")?.textContent ?? null,
        remover: Boolean(item(0)?.querySelector(".moderar-remover")),
        acrescimo: item(0)?.textContent.includes("acréscimo") ?? false,
        vizinha: item(1)?.querySelector(".mensagem-miolo") === null && Boolean(item(1)?.querySelector(".corpo")),
      };
      guardarCasamentos({ casamentos: [{ message: 0, start: 0, end: 8 }], atual: null });
      desenharMensagens();
      const naBusca = {
        miolo: Boolean(item(0)?.querySelector(".mensagem-miolo")),
        realce: Boolean(item(0)?.querySelector(".corpo mark")),
      };
      guardarCasamentos({ casamentos: [], atual: null });
      contribuicoesDosMods.revogar(substituida, dono);
      contribuicoesDosMods.revogar(acrescida, dono);

      // **Uma pendente não recebe o desenho de um MOD.** Ela tem `id` zero — o
      // servidor ainda não a gravou —, e uma substituição sem alvo vale para
      // todas as mensagens. O texto dela e o «enviando…» são do produto.
      mensagens = [
        ...mensagens,
        { id: 0, client_message_id: 77, estado: "Enviando", channel: 1, author: 1, author_nickname: "Alex", at_seconds: agora - 5, body: "ainda indo", own: true, edited: false, attachment: null },
      ];
      const todas = contribuicoesDosMods.registrar(dono, instancia, {
        ponto: "conversa.mensagem", modo: "substituir",
        conteudo: [{ forma: "texto", chave: "c", dentro: "corpo do MOD" }],
      }).handle;
      desenharMensagens();
      const pendente = {
        gravadaComOMod: Boolean(item(0)?.querySelector(".mensagem-miolo")),
        miolo: Boolean(item(2)?.querySelector(".mensagem-miolo")),
        corpo: item(2)?.querySelector(".corpo")?.textContent ?? null,
        selo: Boolean(item(2)?.querySelector(".envio")),
      };
      contribuicoesDosMods.revogar(todas, dono);
      modsCarregados.delete("mod/mensagem");
      mensagens = deAntes;
      desenharMensagens();
      return { cabecas, comOMod, naBusca, pendente };
    }).catch((erro) => ({ erro: erro.message.split("\n")[0] }));
    if (mensagem.erro) falhas.push(`conversa.mensagem: ${mensagem.erro}`);
    else {
      if (mensagem.cabecas[1] !== "Lia") {
        falhas.push("conversa.mensagem: duas pessoas com o mesmo apelido, em seguida, e a segunda saiu sem "
          + `cabeça — o agrupamento é pelo apelido, e não pela chave (${JSON.stringify(mensagem.cabecas)})`);
      }
      const m = mensagem.comOMod;
      if (m.miolo !== "corpo do MOD" || m.corpoNativo) falhas.push(`conversa.mensagem: o corpo não é o do MOD (${JSON.stringify(m)})`);
      if (m.autor !== "Lia") falhas.push(`conversa.mensagem: a cabeça diz «${m.autor}», e o autor é «Lia»`);
      if (!m.remover) falhas.push("conversa.mensagem: o REMOVER saiu com o corpo do MOD");
      if (!m.acrescimo) falhas.push("conversa.mensagem: o acréscimo do MOD não aparece");
      if (!m.vizinha) falhas.push("conversa.mensagem: a substituição de uma mensagem alcançou a vizinha");
      const p = mensagem.pendente;
      if (!p.gravadaComOMod || p.miolo || p.corpo !== "ainda indo" || !p.selo) {
        falhas.push(`conversa.mensagem: uma substituição sem alvo alcançou a pendente, ou a tirou do lugar (${JSON.stringify(p)})`);
      }
      if (mensagem.naBusca.miolo || !mensagem.naBusca.realce) {
        falhas.push(`conversa.mensagem: com a busca marcando, o corpo nativo e o realce não voltaram (${JSON.stringify(mensagem.naBusca)})`);
      }
    }

    // ------------------------------------- atualizar, sem revogar
    //
    // `contribuicoes.atualizar(handle, conteudo)` (R7 do PERFIS; tarefa 5,
    // Step 6): troca o conteúdo **no mesmo nó** — o que o MOD segura dentro
    // dele (foco, rolagem) não se perde —, e a linha de uma pessoa com o
    // cartão atualizado nunca volta à nativa no meio. Revogar e registrar de
    // novo era o único caminho, e passava pelo nativo.
    const atualizar = await page.evaluate(async () => {
      const r = window.testTable.snapshot;
      const quadro = () => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const instancia = new InstanciaDeMod("mod/atualiza", "h", geracaoDaSessao, {
        pedirEncerramento() {}, async encerrou() { return true; }, entregar() {},
      });
      instancia.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set("mod/atualiza", instancia);
      const dono = { id: "mod/atualiza", instancia };
      const texto = (dentro) => [{ forma: "texto", chave: "t", dentro }];

      const naRegiao = contribuicoesDosMods.registrar(dono, instancia, { ponto: "operador.barra", conteudo: texto("versão 1") }).handle;
      desenhar(r);
      await quadro();
      const antes = $("operador-mods").querySelector(".contribuicao-de-mod");
      contribuicoesDosMods.atualizar(naRegiao, dono, texto("versão 2"));
      desenhar(r);
      await quadro();
      const depois = $("operador-mods").querySelector(".contribuicao-de-mod");
      const regiao = { mesmoNo: Boolean(antes) && antes === depois && antes.isConnected, texto: depois?.textContent ?? null };

      const lia = r.voice_rooms.flatMap((sala) => sala.people).find((p) => !p.is_self);
      const cartao = contribuicoesDosMods.registrar(dono, instancia, {
        ponto: "pessoa.cartao", modo: "substituir", alvo: String(lia.id), conteudo: texto("cartão 1"),
      }).handle;
      desenhar(r);
      await quadro();
      const nativas = [];
      const vigia = new MutationObserver((registros) => {
        for (const registro of registros) {
          for (const no of registro.addedNodes) {
            if (no.nodeType === 1 && no.matches("li.pessoa") && !no.classList.contains("pessoa-apresentada")
              && no.textContent.includes(lia.nickname)) nativas.push(no.textContent.slice(0, 40));
          }
        }
      });
      vigia.observe($("lista-roster"), { childList: true, subtree: true });
      contribuicoesDosMods.atualizar(cartao, dono, texto("cartão 2"));
      desenhar(r);
      await quadro();
      vigia.disconnect();
      const linhaDaLia = document.querySelector('li.pessoa-apresentada[data-apresentada-por="mod/atualiza"]');
      const linha = { nativas, cartao: linhaDaLia?.querySelector(".pessoa-miolo")?.textContent ?? null };

      let recusa = "";
      try {
        contribuicoesDosMods.atualizar(cartao, { id: "outro/mod", instancia }, texto("x"));
      } catch (erro) {
        recusa = erro.message;
      }
      contribuicoesDosMods.revogar(naRegiao, dono);
      contribuicoesDosMods.revogar(cartao, dono);
      modsCarregados.delete("mod/atualiza");
      desenhar(r);
      await quadro();
      return { regiao, linha, recusa };
    }).catch((erro) => ({ erro: erro.message.split("\n")[0] }));
    if (atualizar.erro) falhas.push(`atualizar: ${atualizar.erro}`);
    else {
      if (!atualizar.regiao.mesmoNo || atualizar.regiao.texto !== "versão 2") {
        falhas.push(`atualizar: a região não trocou o conteúdo no mesmo nó (${JSON.stringify(atualizar.regiao)})`);
      }
      if (atualizar.linha.nativas.length || atualizar.linha.cartao !== "cartão 2") {
        falhas.push(`atualizar: a linha passou pela nativa, ou não trocou o cartão (${JSON.stringify(atualizar.linha)})`);
      }
      if (!atualizar.recusa.includes("não é de outro/mod")) {
        falhas.push(`atualizar: o handle de outro MOD não foi recusado (${atualizar.recusa || "aceito"})`);
      }
    }

    // ------------------------------------- a região que a casca não tem
    //
    // `regiao-ausente` (rascunho §11 e §7.1; tarefa 5, Step 8): uma
    // contribuição para uma região que a casca não tem não some calada. O MOD
    // recebe o evento `contribuicao` com `motivo: 'regiao-ausente'`, uma vez; o
    // retrato tira a região das `regioes`; e a contribuição continua
    // registrada, e desenha quando a região existe. Na 0.16 nenhuma das oito
    // fica sem casa no celular (o desenho da F8), então a bancada simula uma
    // casca que declara as regiões dela sem a barra de baixo.
    const ausente = await page.evaluate(async () => {
      const r = window.testTable.snapshot;
      const quadro = () => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const entregues = [];
      const instancia = new InstanciaDeMod("mod/ausente", "h", geracaoDaSessao, {
        pedirEncerramento() {}, async encerrou() { return true; },
        entregar(mensagem) { entregues.push(mensagem); },
      });
      instancia.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set("mod/ausente", instancia);
      const dono = { id: "mod/ausente", instancia };
      const todas = cascaDoRetrato().regioes;
      globalThis.CELULAR = Object.freeze({ regioes: new Set(todas.filter((nome) => nome !== "status.barra")) });
      const { handle } = contribuicoesDosMods.registrar(dono, instancia, {
        ponto: "status.barra", conteudo: [{ forma: "texto", chave: "t", dentro: "da barra de baixo" }],
      });
      desenhar(r);
      await quadro();
      desenhar(r);
      await quadro();
      const semARegiao = {
        desenhada: $("telemetria-mods").textContent.includes("da barra de baixo"),
        avisos: entregues.filter((m) => m.nome === "contribuicao" && m.motivo === "regiao-ausente" && m.ponto === "status.barra").length,
        noRetrato: cascaDoRetrato().regioes.includes("status.barra"),
      };
      delete globalThis.CELULAR;
      desenhar(r);
      await quadro();
      const comARegiao = $("telemetria-mods").textContent.includes("da barra de baixo");
      contribuicoesDosMods.revogar(handle, dono);
      modsCarregados.delete("mod/ausente");
      desenhar(r);
      await quadro();
      return { semARegiao, comARegiao };
    }).catch((erro) => ({ erro: erro.message.split("\n")[0] }));
    if (ausente.erro) falhas.push(`regiao-ausente: ${ausente.erro}`);
    else {
      const s = ausente.semARegiao;
      if (s.desenhada) falhas.push("regiao-ausente: a casca não tem a região, e a contribuição foi desenhada assim mesmo");
      if (s.avisos !== 1) falhas.push(`regiao-ausente: o MOD recebeu ${s.avisos} aviso(s) de região ausente, e o certo é um`);
      if (s.noRetrato) falhas.push("regiao-ausente: o retrato diz que a casca tem a região que ela não tem");
      if (!ausente.comARegiao) falhas.push("regiao-ausente: a região voltou, e a contribuição registrada não desenhou");
    }

    // ------------------------------------- a navegação, substituída
    //
    // `servidor.navegacao` substituído (rascunho §3, a regra aprovada em 09/10):
    // o MOD toma **a ordem e os grupos** de cada lista — salas e canais —, e a
    // linha de cada sala e de cada canal continua do produto, com ENTRAR, o
    // renomear e o apagar. Ninguém some: quem não está em grupo vai para
    // OUTRAS SALAS / OUTROS CANAIS. Uma substituição não vira entrada «DOS
    // MODS», e uma declaração com outra forma é recusada pelo nome.
    const navegacao = await page.evaluate(async () => {
      const r = window.testTable.snapshot;
      const quadro = () => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const textos = (id) => [...$(id).children].map((li) =>
        (li.querySelector(".linha-rotulo, .voice_room-nome") ?? li).textContent.trim());
      const antes = { salas: $("lista-voice_rooms").innerHTML, canais: $("lista-linhas").innerHTML };
      const instancia = new InstanciaDeMod("mod/navegacao", "h", geracaoDaSessao, {
        pedirEncerramento() {}, async encerrou() { return true; }, entregar() {},
      });
      instancia.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set("mod/navegacao", instancia);
      let recusa = "";
      try {
        contribuicoesDosMods.registrar({ id: "mod/navegacao" }, instancia, {
          ponto: "servidor.navegacao", modo: "substituir", conteudo: [{ forma: "texto", dentro: "x" }],
        });
      } catch (erro) {
        recusa = erro.message;
      }
      const { handle } = contribuicoesDosMods.registrar({ id: "mod/navegacao" }, instancia, {
        ponto: "servidor.navegacao",
        modo: "substituir",
        conteudo: {
          salas: { grupos: [{ titulo: "MESA 1", ids: [1] }] },
          canais: { grupos: [{ titulo: "MESA 1", ids: [2, 2, 99] }, { titulo: "VAZIO", ids: [98] }] },
        },
      });
      desenhar(r);
      await quadro();
      const com = {
        salas: textos("lista-voice_rooms"),
        canais: textos("lista-linhas"),
        canaisNaTela: $("lista-linhas").querySelectorAll("button[data-linha]").length,
        entrar: $("lista-voice_rooms").querySelectorAll("li.voice_room").length,
        entradasDeMod: !$("lista-mods-navegacao").hidden,
      };
      contribuicoesDosMods.revogar(handle, { id: "mod/navegacao", instancia });
      modsCarregados.delete("mod/navegacao");
      desenhar(r);
      await quadro();
      const depois = { salas: $("lista-voice_rooms").innerHTML, canais: $("lista-linhas").innerHTML };
      return { recusa, com, voltou: depois.salas === antes.salas && depois.canais === antes.canais, canais: r.channels.length };
    }).catch((erro) => ({ erro: erro.message.split("\n")[0] }));
    if (navegacao.erro) {
      falhas.push(`servidor.navegacao: ${navegacao.erro}`);
    } else {
      if (!navegacao.recusa.includes("servidor.navegacao")) {
        falhas.push(`servidor.navegacao: uma substituição com outra forma não foi recusada pelo nome (${navegacao.recusa || "aceita"})`);
      }
      const esperado = ["MESA 1", "combinados", "OUTROS CANAIS", "geral"];
      if (JSON.stringify(navegacao.com.canais) !== JSON.stringify(esperado)) {
        falhas.push(`servidor.navegacao: os canais saíram ${JSON.stringify(navegacao.com.canais)}, e o esperado é ${JSON.stringify(esperado)}`);
      }
      if (JSON.stringify(navegacao.com.salas) !== JSON.stringify(["MESA 1", "Sala principal"])) {
        falhas.push(`servidor.navegacao: as salas saíram ${JSON.stringify(navegacao.com.salas)}`);
      }
      if (navegacao.com.canaisNaTela !== navegacao.canais) {
        falhas.push(`servidor.navegacao: ${navegacao.com.canaisNaTela} de ${navegacao.canais} canais na tela — alguém sumiu`);
      }
      if (navegacao.com.entrar !== 1) falhas.push("servidor.navegacao: a sala perdeu a linha do produto");
      if (navegacao.com.entradasDeMod) falhas.push("servidor.navegacao: a substituição apareceu como entrada «DOS MODS»");
      if (!navegacao.voltou) falhas.push("servidor.navegacao: revogada, e as listas não voltaram ao que eram");
    }

    // ------------------------------------------- o orçamento de cada ponto
    //
    // `perfil`, na tabela de pontos, diz com que orçamento a declaração é
    // montada; `montarContribuicao` montava tudo com o do cartão (64 nós), e o
    // `superficie` de `pessoa.detalhes` nunca valeu (achado da tarefa 5,
    // decidido pela orquestradora em 09/10). E o que passa do teto é dito ao
    // MOD com o perfil, o teto e o tamanho — e não só com a contagem.
    const orcamento = await page.evaluate(() => {
      const entregues = [];
      const instancia = new InstanciaDeMod("mod/orcamento", "h", geracaoDaSessao, {
        pedirEncerramento() {},
        async encerrou() { return true; },
        entregar(mensagem) { entregues.push(mensagem); },
      });
      instancia.estado = ESTADOS_DE_MOD.ativa;
      modsCarregados.set("mod/orcamento", instancia);
      const montar = (ponto, alvo, linhas) => {
        const { handle } = contribuicoesDosMods.registrar({ id: "mod/orcamento" }, instancia, {
          ponto, modo: "adicionar", alvo,
          conteudo: Array.from({ length: linhas }, (_, i) => ({ forma: "texto", chave: `l${i}`, dentro: `linha ${i}` })),
        });
        const contribuicao = contribuicoesDosMods.porHandle.get(handle);
        const no = montarContribuicao(contribuicao, alvo);
        const linhasNaTela = no ? [...no.querySelectorAll("*")].filter((n) => /^linha \d+$/.test(n.textContent)).length : 0;
        contribuicoesDosMods.revogar(handle, { id: "mod/orcamento", instancia });
        return linhasNaTela;
      };
      const naRegiao = montar("operador.barra", "", 40);
      const avisoDaRegiao = entregues.splice(0).find((m) => m.nome === "contribuicao") ?? null;
      const nosDetalhes = montar("pessoa.detalhes", "12", 40);
      const avisoDosDetalhes = entregues.splice(0).find((m) => m.nome === "contribuicao") ?? null;
      modsCarregados.delete("mod/orcamento");
      return { naRegiao, avisoDaRegiao, nosDetalhes, avisoDosDetalhes };
    });
    // Uma linha de texto custa dois nós: 40 linhas são 80, e o cartão tem 64.
    // Montam 32, e as 8 que sobram são recusadas inteiras, com o texto junto.
    if (orcamento.naRegiao !== 32) {
      falhas.push(`orçamento: uma região montou ${orcamento.naRegiao} de 40 linhas, e o cartão (64 nós) monta 32`);
    }
    const aviso = orcamento.avisoDaRegiao;
    if (!aviso || aviso.motivo !== "orcamento" || aviso.perfil !== "cartao" || aviso.teto !== 64
      || aviso.tamanho !== 80 || aviso.recusados !== 8) {
      falhas.push("orçamento: o MOD não soube, com o perfil, o teto e o tamanho, que a declaração "
        + `passou do teto — recebeu ${JSON.stringify(aviso)}`);
    }
    if (orcamento.nosDetalhes !== 40 || orcamento.avisoDosDetalhes !== null) {
      falhas.push(`orçamento: pessoa.detalhes (perfil «superficie», 4096 nós) montou ${orcamento.nosDetalhes} `
        + `de 40 linhas, com o aviso ${JSON.stringify(orcamento.avisoDosDetalhes)}`);
    }
  } finally {
    await browser.close();
    server.close();
  }
  if (falhas.length) {
    console.error(`regioes-do-produto: ${falhas.length} falha(s)`);
    for (const falha of falhas) console.error(`  ${falha}`);
    process.exit(1);
  }
  console.log(`regioes-do-produto: ok (${REGIOES.map((r) => r.ponto).join(", ")})`);
})().catch((erro) => {
  console.error(erro.message ?? erro);
  process.exit(1);
});
