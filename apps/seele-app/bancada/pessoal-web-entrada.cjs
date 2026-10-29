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
    if (req.url === '/') { res.setHeader('Content-Type', 'text/html; charset=utf-8'); res.end('<!doctype html><meta name="viewport" content="width=device-width"><button id="boot-conversa-pessoal" hidden>CONVERSAR POR LINK</button><span id="boot-perfil-nome">Pessoa</span><script src="/glifos.js"></script><script src="/pessoal-web.js"></script><script src="/pessoal-web-entrada.js"></script>'); return; }
    const nome = req.url.slice(1);
    if (!['glifos.js','pessoal-web.js','pessoal-web-entrada.js'].includes(nome)) { res.statusCode = 404; res.end(); return; }
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
        window.carregarOWasmDaPagina = async () => ({ bytes: new ArrayBuffer(0) });
        window.temCapturaParaCodificar = () => false;
        window.criarVozDoNavegador = () => {
          let geracao = 0;
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
        window.criarCompartilhamentoPessoalWeb = () => ({
          fechar: () => { prova.fechouTela++; }, receberControle() {},
        });
        window.criarServicoDeChamadasWeb = async callbacks => {
          window.callbacksDaChamada = callbacks;
          return {
            wasm: {}, receberPorCodigo: async () => { prova.recebeu++; if (prova.esperarRegistro) await new Promise((ok, nao) => { window.rejeitarRegistro = () => nao(new DOMException("Cancelada", "AbortError")); }); }, criarConvite: async () => {
              if (prova.esperarCriacao) await new Promise((ok, nao) => { window.rejeitarCriacao = () => nao(new Error('encontro indisponível')); });
              callbacks.aoEstado({ estado: 'recebendo' }); return { link: 'https://example.test/#chamada/teste' };
            },
            abrirConvite: () => { prova.abriuConvite++; }, chamar: async () => {
              if (prova.esperarChamada) await new Promise((ok, nao) => { window.rejeitarChamada = () => nao(new Error('conexão recusada')); });
            }, atender: async () => {},
            destruir: () => { prova.destruiu++; }, encerrar: () => callbacks.aoEstado({ estado: 'encerrada' }),
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
      assert.equal(await page.evaluate(() => prova.capturas), 0, 'tocar/aceitar não solicita captura antes do botão de microfone');
      await page.evaluate(() => {
        callbacksDaChamada.aoEstado({ estado: 'dentro', papel: 'quem-recebe' });
        callbacksDaChamada.aoPar({ transporteDaVoz: {}, tela: {}, abortar() {} }, { papel: 'quem-recebe' });
      });
      await page.waitForFunction(() => prova.liberouFoco === 1 && prova.entrouVoz === 1);
      assert.equal(await page.evaluate(() => prova.capturas), 0, 'a escuta não deve pedir microfone');
      const fechamentosAntes = await page.evaluate(() => prova.fechouVoz);
      // IPC de foco falha antes da permissão: cancela captura pendente sem
      // interromper a reprodução e sem promessa rejeitada sem dono.
      await page.evaluate(() => prova.falharFoco = true);
      await page.getByRole('button', { name: 'LIGAR MICROFONE' }).click();
      await page.waitForFunction(() => prova.calou === 1);
      await page.evaluate(() => permitirMicrofone());
      assert.equal(await page.evaluate(() => prova.trilhasVivas), 0);
      assert.equal(await page.evaluate(() => prova.fechouVoz), fechamentosAntes);
      await page.evaluate(() => { prova.falharFoco = false; prova.esperarFoco = true; });
      await page.getByRole('button', { name: 'LIGAR MICROFONE' }).click();
      await page.waitForFunction(() => typeof permitirFoco === 'function');
      await page.evaluate(() => recusarMicrofone());
      await page.waitForFunction(() => prova.calou === 2);
      await page.evaluate(() => { permitirFoco(); prova.esperarFoco = false; });
      assert.deepEqual(await page.evaluate(() => prova.rejeicoes), []);
      await page.getByRole('button', { name: 'LIGAR MICROFONE' }).click();
      await page.waitForFunction(() => prova.capturas === 3);
      await page.getByRole('button', { name: 'SAIR DA CONVERSA' }).click();
      await page.evaluate(() => permitirMicrofone());
      await page.waitForFunction(base => prova.fechouVoz >= base + 2, fechamentosAntes);
      assert.equal(await page.evaluate(() => prova.entrouVoz), 1, 'a permissão tardia reabriu voz após sair');
      assert.ok(await page.evaluate(() => prova.fechouTela >= 1));
      assert.equal(await page.getByRole('button', { name: 'DESLIGAR MICROFONE' }).count(), 0);
      const antesDeSuspender = await page.evaluate(() => ({ destruiu: prova.destruiu, recebeu: prova.recebeu }));
      await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent('pagehide')));
      await page.waitForFunction(base => prova.destruiu > base, antesDeSuspender.destruiu);
      await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true })));
      await page.waitForFunction(base => prova.recebeu > base, antesDeSuspender.recebeu);
      assert.equal(await page.evaluate(() => prova.capturas), 3, 'BFCache não reabre microfone');
      console.log(`${engine}: criação/entrada recusadas visíveis, cliques pendentes bloqueados, cancelamento, foco recusado, captura recusada, permissão tardia, saída e BFCache PASS`);
    } finally { await browser.close(); }
  } } finally { server.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
