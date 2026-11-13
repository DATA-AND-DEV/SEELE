// Exercita o ciclo da UI com serviço/mídia controlados. Permissão tardia não
// reativa microfone depois de sair; fechar página desmonta todos os recursos.
const assert = require('node:assert/strict');
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const engines = require('./playwright.cjs');
const ui = path.resolve(__dirname, '../ui');
(async () => {
  const server = http.createServer((req, res) => {
    if (req.url === '/') { res.setHeader('Content-Type', 'text/html; charset=utf-8'); res.end('<!doctype html><meta name="viewport" content="width=device-width"><button id="boot-conversa-pessoal" hidden>CONVERSAR POR LINK</button><span id="boot-perfil-nome">Pessoa</span><script src="/glifos.js"></script><script src="/frases.js"></script><script src="/alertas-sonoros.js"></script><script src="/pessoal-web.js"></script><script src="/pessoal-web-entrada.js"></script>'); return; }
    const nome = req.url.slice(1);
    if (!["glifos.js","frases.js","alertas-sonoros.js","pessoal-web.js","pessoal-web-entrada.js"].includes(nome)) { res.statusCode = 404; res.end(); return; }
    res.setHeader('Content-Type', 'text/javascript; charset=utf-8'); res.end(fs.readFileSync(path.join(ui, nome)));
  });
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  try { for (const engine of ['chromium', 'webkit']) {
    const browser = await engines[engine]().launch({ headless: true });
    try {
      const page = await browser.newPage();
      await page.addInitScript(() => {
        window.PONTE_NO_LUGAR_DO_TAURI = true;
        // Esta bancada usa adaptador de voz controlado; capacidade é um vetor.
        window.AudioEncoder ||= function() {}; window.AudioDecoder ||= function() {};
        window.prova = { fechouVoz: 0, entrouVoz: 0, fechouTela: 0, destruiu: 0, liberouFoco: 0, recebeu: 0, capturas: 0, calou: 0, trilhasVivas: 0, abriuConvite: 0, rejeicoes: [] };
        window.addEventListener("unhandledrejection", e => prova.rejeicoes.push(String(e.reason)));
        window.estadoPessoalDaPonte = null;
        window.emitirNaPonte = () => {};
        window.preferenciaDeReceberPessoalWeb = async () => true;
        window.invocarNaPonte = async cmd => {
          if (cmd !== 'focar_voz') return;
          prova.liberouFoco++;
          if (prova.falharFoco) throw new Error('foco recusado');
          if (prova.esperarFoco) await new Promise(ok => window.permitirFoco = ok);
        };
        window.carregarOWasmDaPagina = async () => {
          if (prova.esperarWasm) await new Promise(ok => { window.entregarWasm = ok; });
          return { bytes: new ArrayBuffer(0) };
        };
        window.temCapturaParaCodificar = () => false;
        window.criarVozDoNavegador = () => {
          let geracao = 0;
          prova.criouVoz = (prova.criouVoz || 0) + 1;
          return {
          ligar: pedido => {
            if (pedido?.somenteOuvir) return Promise.resolve();
            prova.capturas++;
            const vez = geracao;
            return new Promise((ok, nao) => {
              window.permitirMicrofone = () => { if (vez === geracao) prova.trilhasVivas++; ok(); };
              window.recusarMicrofone = () => nao(new Error('microfone recusado'));
            });
          },
          calarMicrofone() { geracao++; prova.calou++; prova.trilhasVivas = 0; }, mudo() {},
          desligar: async () => { geracao++; prova.trilhasVivas = 0; prova.fechouVoz++; }, modo() {},
          entrar: () => { prova.entrouVoz++; }, contas: async () => ({ pagina: { falhas: [] } }),
        }; };
        window.criarCompartilhamentoPessoalWeb = ({aoEstado}) => {
          let noAr = false;
          return {
            get noAr() { return noAr; },
            compartilhar() { prova.compartilhou = (prova.compartilhou || 0) + 1; noAr = true; aoEstado("transmitindo"); },
            parar() { prova.parouTela = (prova.parouTela || 0) + 1; noAr = false; aoEstado("parada"); },
            fechar: () => { prova.fechouTela++; }, receberControle() {},
          };
        };
        window.criarServicoDeChamadasWeb = async callbacks => {
          window.callbacksDaChamada = callbacks;
          // Como o serviço de verdade (`cancelarAtual` em `chamada-web.js`):
          // encerrar só avisa «encerrada» quando HÁ uma conversa para encerrar.
          // Um `encerrar` que avisava sempre escondia o defeito de 13/11.
          let haConversa = false;
          return window.servicoDeProva = {
            wasm: {}, receberPorCodigo: async () => { prova.recebeu++; if (prova.esperarRegistro) await new Promise((ok, nao) => { window.rejeitarRegistro = () => nao(new DOMException("Cancelada", "AbortError")); }); }, criarConvite: async () => {
              if (prova.esperarCriacao) await new Promise((ok, nao) => { window.rejeitarCriacao = () => nao(new Error('encontro indisponível')); });
              haConversa = true; callbacks.aoEstado({ estado: 'recebendo' }); return { link: 'https://example.test/#chamada/teste' };
            },
            chamarCodigo: async () => { haConversa = true; prova.chamouCodigo = (prova.chamouCodigo || 0) + 1; return {}; },
            abrirConvite: () => { prova.abriuConvite++; }, chamar: async () => {
              haConversa = true;
              if (prova.esperarChamada) await new Promise((ok, nao) => { window.rejeitarChamada = () => nao(new Error('conexão recusada')); });
            }, atender: async () => { haConversa = true; },
            destruir: () => { prova.destruiu++; },
            encerrar: () => { if (!haConversa) return; haConversa = false; callbacks.aoEstado({ estado: 'encerrada' }); },
            revogarConvite: async () => {},
          };
        };
      });
      await page.goto(`http://127.0.0.1:${server.address().port}`);
      await page.waitForFunction(() => prova.recebeu >= 1);
      await page.evaluate(() => { prova.esperarRegistro = true; window.registroAntigo = seelePessoalWeb.receber(true); });
      await page.waitForFunction(() => typeof rejeitarRegistro === 'function');
      await page.evaluate(() => seelePessoalWeb.receber(false));
      await page.evaluate(async () => { rejeitarRegistro(); await registroAntigo; });
      assert.equal(await page.evaluate(() => seelePessoalWeb.estadoDaSala.estado), 'Desligada', 'registro antigo não deve trocar Desligada por Falhou');
      const registrosAntesDoLink = await page.evaluate(() => prova.recebeu);
      await page.evaluate(() => location.hash = '#chamada/teste');
      await page.waitForFunction(() => prova.abriuConvite === 1);
      assert.equal(await page.evaluate(() => prova.recebeu), registrosAntesDoLink, 'abrir convite não deve restaurar a preferência inicial já desligada');
      await page.evaluate(async () => { prova.esperarRegistro = false; await seelePessoalWeb.receber(true); });
      await page.locator('#boot-conversa-pessoal').click();
      await page.getByLabel('Seu nome', { exact: true }).fill('Nome escolhido');
      await page.evaluate(() => prova.esperarCriacao = true);
      await page.getByRole('button', { name: 'CRIAR CONVERSA', exact: true }).click();
      await page.waitForFunction(() => typeof rejeitarCriacao === 'function');
      assert.equal(await page.getByRole('button', { name: 'CRIAR CONVERSA', exact: true }).isDisabled(), true, 'limpar a conversa não pode liberar outro clique durante a criação');
      await page.evaluate(() => rejeitarCriacao());
      await page.getByRole('alert').waitFor({ state: 'visible' });
      assert.match(await page.getByRole('alert').textContent(), /Não foi possível concluir/);
      assert.equal(await page.getByRole('button', { name: 'CRIAR CONVERSA', exact: true }).isEnabled(), true);
      // Sair durante outra tentativa continua invalidando seu erro tardio.
      await page.getByRole('button', { name: 'CRIAR CONVERSA', exact: true }).click();
      await page.getByRole('button', { name: 'VOLTAR', exact: true }).click();
      await page.evaluate(() => rejeitarCriacao());
      await page.locator('#boot-conversa-pessoal').click();
      assert.equal(await page.getByRole('alert').isVisible(), false);
      await page.evaluate(() => prova.esperarCriacao = false);
      await page.evaluate(() => { prova.esperarChamada = true; location.hash = '#chamada/outro'; });
      await page.getByRole('button', { name: 'ENTRAR NA CONVERSA', exact: true }).click();
      await page.waitForFunction(() => typeof rejeitarChamada === 'function');
      assert.equal(await page.getByRole('button', { name: 'ENTRAR NA CONVERSA', exact: true }).isDisabled(), true);
      await page.evaluate(() => rejeitarChamada());
      await page.getByRole('alert').waitFor({ state: 'visible' });
      assert.match(await page.getByRole('alert').textContent(), /Não foi possível concluir/);
      await page.getByRole('button', { name: 'VOLTAR', exact: true }).click();
      await page.locator('#boot-conversa-pessoal').click();
      await page.getByRole('button', { name: 'CRIAR CONVERSA', exact: true }).click();
      await page.evaluate(() => callbacksDaChamada.aoEstado({ estado: 'tocando', apelido: 'Par' }));
      assert.equal(await page.getByLabel('Seu nome', { exact: true }).inputValue(), 'Nome escolhido', 'receber a chamada não troca o nome escolhido pelo perfil padrão');
      // 0.19: entrar na conversa pede o microfone NO GESTO (o teste no Chrome do
      // Android: a pessoa entrava e ficava sem falar até achar o botão).
      const capturasAntes = await page.evaluate(() => prova.capturas);
      await page.getByRole('button', { name: 'ATENDER', exact: true }).click();
      assert.equal(await page.evaluate(() => prova.capturas), capturasAntes + 1, 'atender a conversa não pediu o microfone no gesto');
      await page.evaluate(() => {
        callbacksDaChamada.aoEstado({ estado: 'dentro', papel: 'quem-recebe' });
        callbacksDaChamada.aoPar({ transporteDaVoz: {}, tela: {}, abortar() {} }, { papel: 'quem-recebe' });
      });
      await page.waitForFunction(() => prova.liberouFoco === 1 && prova.entrouVoz === 1);
      // A escuta entra antes da permissão; o microfone espera por ela, sem pedir outra captura.
      assert.equal(await page.getByRole('button', { name: 'DESLIGAR MICROFONE' }).count(), 0, 'o microfone apareceu ligado antes da permissão');
      await page.evaluate(() => permitirMicrofone());
      await page.getByRole('button', { name: 'DESLIGAR MICROFONE' }).waitFor({ state: 'visible' });
      assert.equal(await page.evaluate(() => prova.capturas), capturasAntes + 1, 'o par que chegou pediu outra captura');
      assert.equal(await page.evaluate(() => prova.trilhasVivas), 1, 'a permissão do gesto não ligou o microfone');
      assert.equal(await page.evaluate(() => prova.entrouVoz), 2, 'o microfone do gesto não entrou na voz');
      // Compartilhar, trocar sem confundir com parar, e parar por controle próprio.
      await page.evaluate(() => { window.temCapturaParaCodificar = () => true; callbacksDaChamada.aoEstado({ estado: 'dentro', papel: 'quem-recebe' }); });
      await page.getByRole('button', {name:'COMPARTILHAR TELA',exact:true}).click();
      await page.getByRole('button', {name:'TROCAR TELA',exact:true}).click();
      assert.equal(await page.evaluate(()=>prova.compartilhou),2);
      assert.equal(await page.evaluate(()=>prova.parouTela||0),0);
      await page.getByRole('button', {name:'PARAR COMPARTILHAMENTO',exact:true}).click();
      assert.equal(await page.evaluate(()=>prova.parouTela),1);
      assert.equal(await page.getByRole('button', {name:'TROCAR TELA',exact:true}).count(),0);
      // Desligar pelo botão volta ao caminho de sempre: o resto da bancada parte do microfone fechado.
      await page.getByRole('button', { name: 'DESLIGAR MICROFONE' }).click();
      await page.waitForFunction(() => prova.calou === 1);
      const fechamentosAntes = await page.evaluate(() => prova.fechouVoz);
      // IPC de foco falha antes da permissão: cancela captura pendente sem
      // interromper a reprodução e sem promessa rejeitada sem dono.
      await page.evaluate(() => prova.falharFoco = true);
      await page.getByRole('button', { name: 'LIGAR MICROFONE' }).click();
      await page.waitForFunction(() => prova.calou === 2);
      await page.evaluate(() => permitirMicrofone());
      assert.equal(await page.evaluate(() => prova.trilhasVivas), 0);
      assert.equal(await page.evaluate(() => prova.fechouVoz), fechamentosAntes);
      await page.evaluate(() => { prova.falharFoco = false; prova.esperarFoco = true; });
      await page.getByRole('button', { name: 'LIGAR MICROFONE' }).click();
      await page.waitForFunction(() => typeof permitirFoco === 'function');
      await page.evaluate(() => recusarMicrofone());
      await page.waitForFunction(() => prova.calou === 3);
      await page.evaluate(() => { permitirFoco(); prova.esperarFoco = false; });
      assert.deepEqual(await page.evaluate(() => prova.rejeicoes), []);
      await page.getByRole('button', { name: 'LIGAR MICROFONE' }).click();
      await page.waitForFunction(base => prova.capturas === base + 4, capturasAntes);
      await page.getByRole('button', { name: 'SAIR DA CONVERSA' }).click();
      await page.evaluate(() => permitirMicrofone());
      await page.waitForFunction(base => prova.fechouVoz >= base + 2, fechamentosAntes);
      assert.equal(await page.evaluate(() => prova.entrouVoz), 2, 'a permissão tardia reabriu voz após sair');
      assert.ok(await page.evaluate(() => prova.fechouTela >= 1));
      assert.equal(await page.getByRole('button', { name: 'DESLIGAR MICROFONE' }).count(), 0);
      const antesDeSuspender = await page.evaluate(() => ({ destruiu: prova.destruiu, recebeu: prova.recebeu }));
      await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent('pagehide')));
      await page.waitForFunction(base => prova.destruiu > base, antesDeSuspender.destruiu);
      await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true })));
      await page.waitForFunction(base => prova.recebeu > base, antesDeSuspender.recebeu);
      assert.equal(await page.evaluate(() => prova.capturas), capturasAntes + 4, 'BFCache não reabre microfone');
      // **Quem liga pelo código fala também na segunda conversa.** Medido em
      // página publicada em 13/11/2026 (1.1.0), com duas páginas: quem saía de
      // uma conversa e fechava a tela (VOLTAR) ficava sem o objeto da voz, e a
      // ligação seguinte dele conectava com o microfone fechado, 0 pacotes de
      // voz em 4 s, e a frase que avisava era apagada na mesma hora.
      const ligarPeloCodigo = async (volta) => {
        const base = await page.evaluate(() => ({ capturas: prova.capturas, entrou: prova.entrouVoz }));
        await page.evaluate(() => { window.ligacao = seelePessoalWeb.chamarCodigo({ codigo: 'AAAA-BBBB-CCCC-DDDD', apelido: 'Nome escolhido' }); });
        await page.waitForFunction(b => prova.capturas > b, base.capturas, { timeout: 5000 })
          .catch(() => { throw new Error(`${engine}: a ${volta} ligação pelo código não pediu o microfone: quem liga entra mudo`); });
        await page.evaluate(async () => {
          await ligacao;
          callbacksDaChamada.aoEstado({ estado: 'dentro', papel: 'quem-liga' });
          callbacksDaChamada.aoPar({ transporteDaVoz: {}, tela: {}, abortar() {} }, { papel: 'quem-liga' });
        });
        await page.waitForFunction(b => prova.entrouVoz > b, base.entrou);
        await page.evaluate(() => permitirMicrofone());
        await page.getByRole('button', { name: 'DESLIGAR MICROFONE' }).waitFor({ state: 'visible', timeout: 5000 })
          .catch(() => { throw new Error(`${engine}: na ${volta} ligação pelo código o microfone não ligou com a chegada do par`); });
        assert.equal(await page.evaluate(() => prova.trilhasVivas), 1, `${volta} ligação: o microfone do gesto não ficou vivo`);
      };
      // Fechar a tela sem conversa nenhuma: a voz é refeita, o serviço fica.
      const vozesAntes = await page.evaluate(() => prova.criouVoz);
      await page.getByRole('button', { name: 'VOLTAR', exact: true }).click();
      await page.waitForFunction(b => prova.criouVoz > b, vozesAntes, { timeout: 5000 })
        .catch(() => { throw new Error(`${engine}: fechar a tela da conversa deixou a página sem a voz, com o serviço de pé`); });
      await ligarPeloCodigo('primeira');
      // Sair e fechar a tela, como qualquer pessoa faz ao terminar de conversar.
      await page.getByRole('button', { name: 'SAIR DA CONVERSA' }).click();
      await page.getByRole('button', { name: 'VOLTAR', exact: true }).click();
      await ligarPeloCodigo('segunda');
      // E quando o OUTRO lado desliga, e esta pessoa só fecha a tela.
      await page.evaluate(() => { callbacksDaChamada.aoPar(null, { papel: 'quem-liga' }); callbacksDaChamada.aoEstado({ estado: 'encerrada' }); servicoDeProva.encerrar(); });
      await page.getByRole('button', { name: 'VOLTAR', exact: true }).click();
      await ligarPeloCodigo('terceira');
      // **Ligar enquanto a voz ainda nasce espera por ela**, em vez de entrar
      // mudo: o `.wasm` demora, a pessoa já apertou, e a captura vem quando dá.
      await page.getByRole('button', { name: 'SAIR DA CONVERSA' }).click();
      await page.evaluate(() => { prova.esperarWasm = true; });
      await page.getByRole('button', { name: 'VOLTAR', exact: true }).click();
      await page.waitForFunction(() => typeof entregarWasm === 'function');
      const antesDaEspera = await page.evaluate(() => ({ capturas: prova.capturas, entrou: prova.entrouVoz }));
      await page.evaluate(() => { window.ligacao = seelePessoalWeb.chamarCodigo({ codigo: 'AAAA-BBBB-CCCC-DDDD', apelido: 'Nome escolhido' }); });
      await page.waitForTimeout(200);
      assert.equal(await page.evaluate(() => prova.capturas), antesDaEspera.capturas, 'a captura abriu antes de a voz existir');
      const reclamou = await page.evaluate(() => [...document.querySelectorAll('[role=alert]')].filter(e => !e.hidden && e.textContent.trim()).map(e => e.textContent.trim()));
      assert.deepEqual(reclamou, [], `${engine}: a voz ainda nasce e a tela já reclama em vez de esperar`);
      await page.evaluate(() => { prova.esperarWasm = false; entregarWasm(); });
      await page.waitForFunction(b => prova.capturas > b, antesDaEspera.capturas, { timeout: 5000 })
        .catch(() => { throw new Error(`${engine}: quem ligou enquanto a voz nascia ficou sem o microfone depois que ela ficou pronta`); });
      await page.evaluate(async () => {
        await ligacao;
        callbacksDaChamada.aoEstado({ estado: 'dentro', papel: 'quem-liga' });
        callbacksDaChamada.aoPar({ transporteDaVoz: {}, tela: {}, abortar() {} }, { papel: 'quem-liga' });
      });
      await page.waitForFunction(b => prova.entrouVoz > b, antesDaEspera.entrou);
      await page.evaluate(() => permitirMicrofone());
      await page.getByRole('button', { name: 'DESLIGAR MICROFONE' }).waitFor({ state: 'visible', timeout: 5000 })
        .catch(() => { throw new Error(`${engine}: a voz que nasceu depois do gesto não ligou o microfone com a chegada do par`); });
      assert.deepEqual(await page.evaluate(() => prova.rejeicoes), []);
      console.log(`${engine}: criação/entrada recusadas visíveis, cliques pendentes bloqueados, microfone pedido no gesto e ligado com o par, cancelamento, foco recusado, captura recusada, permissão tardia, saída, BFCache e microfone de quem liga pelo código em três conversas seguidas PASS`);
    } finally { await browser.close(); }
  } } finally { server.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
