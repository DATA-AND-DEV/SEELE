// diag-0151 (H): a jornada do relato 0.15.1 → 0.16 na UI DE VERDADE (index.html,
// CSS e JS reais), com a ponte do Tauri devolvendo as respostas GRAVADAS do
// backend real da 0.16.0 (o teste `reproducao_0151::b_*` sobre a fixture da
// 0.15.1, docs/evidencias/diagnostico-0151/v016/reproducao_0151.rs, as escreve;
// as de 24/10 estão em docs/evidencias/diagnostico-0151/respostas-0.16/). Numa
// árvore da 0.16 exportada, passe a pasta como primeiro argumento.
//
// O que é real e o que não é:
//   - real: o HTML/CSS/JS da pasta ../ui ao lado deste arquivo, o motor do
//     navegador (Chromium; WebKit com --webkit), o setInterval da porta, os
//     cliques;
//   - gravado do backend real: `hospedar` (o Anfitriao montado com o convite do
//     servidor que subiu), o erro do `connect` do dono (ModsNaoAceitos), o
//     `estado_da_porta` antes e depois da visita bater, e `pedidos_da_portaria`;
//   - simulado: todo o resto da ponte (o padrão de `telas.cjs`), e a sequência
//     dos casos de silêncio da faixa (P6), que troca a fila à mão.
//
// O hospedar é chamado pela função `hospedar()` da tela-boot.js, que é onde o
// HOSPEDAR AQUI termina (tela-boot.js:1032-1035 → abrirHospedagem → hospedar).
//
// Requisitos (0.17, E7) que esta bancada confere, e que REPROVAM no defeito:
//   R1  com o servidor hospedado e a UI fora da sessão, a faixa de quem bate aparece;
//   R2  VER QUEM É mostra a janela da porta VISÍVEL, com o pedido, em qualquer tela;
//   R3  decidir um pedido e chegar outro com a mesma contagem não cala o aviso;
//   R4  com o servidor de pé e a UI na tela inicial, há indicação de que se hospeda.
//
//   node apps/seele-app/bancada/diagnostico-0151.cjs [pasta-das-respostas] [--webkit] [--capturas <pasta>]
//
// Sai com código 1 se algum requisito reprova; imprime o relatório em JSON.

const fs = require('node:fs');
const path = require('node:path');
const { servir, respostas } = require('./telas.cjs');
const pw = require('./playwright.cjs');

const args = process.argv.slice(2);
const webkit = args.includes('--webkit');
const iCapt = args.indexOf('--capturas');
const capturas = iCapt >= 0 ? args[iCapt + 1] : null;
const pastaRespostas = args.find((a, i) => !a.startsWith('--') && (iCapt < 0 || i !== iCapt + 1))
  // As gravadas em 24/10, no repositório (docs/evidencias/diagnostico-0151/respostas-0.16).
  ?? path.resolve(__dirname, '../../../docs/evidencias/diagnostico-0151/respostas-0.16');
const ler = (nome) => JSON.parse(fs.readFileSync(path.join(pastaRespostas, `${nome}.json`), 'utf8'));

const GRAVADO = {
  hospedar: ler('hospedar'),
  connectDono: ler('connect-dono'),
  porta0: ler('estado_da_porta-0'),
  porta1: ler('estado_da_porta-1'),
  pedidos1: ler('pedidos_da_portaria-1'),
};

(async () => {
  const relatorio = { ui: path.resolve(__dirname, '../ui'), motor: webkit ? 'webkit' : 'chromium', respostas: pastaRespostas, passos: [], requisitos: {} };
  const passo = (nome, visto) => { relatorio.passos.push({ nome, ...visto }); };
  const server = servir();
  await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
  // O playwright.cjs da 0.16 só exporta o chromium; o WebKit vem da mesma instalação.
  const motor = webkit ? require(process.env.PLAYWRIGHT ?? 'playwright').webkit : pw.chromium();
  const browser = await motor.launch({ headless: true });
  const erros = [];
  try {
    const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
    page.on('pageerror', (e) => erros.push(e.message));
    await page.addInitScript((tabela) => {
      window.testTable = tabela;
      window.testCalls = [];
      window.__TAURI__ = {
        core: { invoke: async (cmd, args) => {
          testCalls.push({ cmd, args: JSON.parse(JSON.stringify(args ?? null)) });
          const r = testTable[cmd];
          return typeof r === 'function' ? r(args) : (r ?? null);
        } },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
      };
    }, respostas({}));
    await page.goto(`http://127.0.0.1:${server.address().port}`);

    await page.evaluate((g) => {
      // Antes de hospedar: não hospeda.
      testTable.estado_da_porta = { ...g.porta0, hospedando: false, pendentes: 0 };
      testTable.pedidos_da_portaria = [];
      testTable.conferir_esta_versao = null;
      testTable.onde_estou = 'Guardado';
      testTable.meu_servidor = null;
      testTable.hospedar = () => {
        // A partir daqui o servidor está de pé (gravado do backend real).
        testTable.estado_da_porta = g.porta0;
        // `meu_servidor` (main.rs, struct MeuServidor { alvo, nome }): o que o
        // Rust responde com o guardado no ar. SIMULADO no formato, com o alvo gravado.
        testTable.meu_servidor = { alvo: g.hospedar.aqui, nome: null };
        return g.hospedar;
      };
      testTable.connect = () => { throw g.connectDono; };
      window.visivel = (id) => {
        const el = document.getElementById(id);
        if (!el) return null;
        const r = el.getBoundingClientRect();
        const vis = typeof el.checkVisibility === 'function' ? el.checkVisibility() : r.width > 0;
        return { hidden: el.hidden, visivel: vis && r.width > 0 && r.height > 0, largura: Math.round(r.width), altura: Math.round(r.height) };
      };
      window.telaNaFrente = () => [...document.querySelectorAll('.tela')].filter((t) => !t.hidden).map((t) => t.id);
    }, GRAVADO);

    const foto = async (nome) => {
      if (!capturas) return;
      fs.mkdirSync(capturas, { recursive: true });
      await page.screenshot({ path: path.join(capturas, `${nome}.png`) });
    };
    const estado = () => page.evaluate(() => ({
      telas: telaNaFrente(),
      aceiteDeMods: visivel('mods-aceite'),
      faixa: visivel('portaria-batendo'),
      portaria: visivel('portaria'),
      bootErro: document.getElementById('boot-erro')?.textContent ?? null,
      chamadas: testCalls.map((c) => c.cmd).filter((c) => !['estado_da_porta'].includes(c)).slice(-12),
    }));

    // P1. Hospedar (hospedar → abrirPorta → conectar → ModsNaoAceitos).
    const temHospedar = await page.evaluate(() => typeof hospedar === 'function');
    if (!temHospedar) {
      passo('P1 hospedar', { aplicavel: false, motivo: 'esta UI não tem a função global hospedar()' });
    } else {
      await page.evaluate(() => { servidorEscolhido = null; return hospedar(); }).catch((e) => erros.push(String(e)));
      await page.waitForTimeout(800);
      // O diálogo «porta» (o link) abre por cima; fechá-lo como a pessoa faria.
      passo('P1 hospedar: servidor de pé, connect do dono → ModsNaoAceitos', await estado());
      await foto('p1-depois-de-hospedar');

      // P2. A pessoa recusa a pergunta dos MODs do próprio servidor.
      const recusou = await page.evaluate(() => {
        const b = document.getElementById('mods-recusar');
        if (b && b.getClientRects().length > 0) { b.click(); return true; }
        return false;
      });
      await page.evaluate(() => { const p = document.getElementById('porta'); if (p && !p.hidden) document.getElementById('porta-fechar')?.click(); });
      await page.waitForTimeout(500);
      passo('P2 recusar a pergunta dos MODs', { recusou, ...(await estado()) });
      await foto('p2-recusou-os-mods');

      // R4: hospedando e fora da sessão, algo diz que se hospeda?
      const r4 = await page.evaluate(() => {
        const texto = [...document.querySelectorAll('.tela:not([hidden]), body > :not(.tela)')]
          .filter((el) => el.getClientRects().length > 0).map((el) => el.innerText).join('\n');
        // Um ESTADO, e não o texto fixo da entrada («ponha o seu no ar»), que
        // casava com a regex antiga e aprovava a tela vazia (medido em 24/10).
        return { menciona: /HOSPEDANDO|est[aá] no ar|voc[eê] (est[aá] )?hosped|seu servidor est[aá]/i.test(texto) };
      });
      relatorio.requisitos.R4_indicacao_de_que_hospeda_fora_da_sessao = r4.menciona;
    }

    // P3. Alguém bate (gravado: pendentes 1).
    await page.evaluate((g) => { testTable.estado_da_porta = g.porta1; testTable.pedidos_da_portaria = g.pedidos1; }, GRAVADO);
    await page.waitForFunction(() => visivel('portaria-batendo')?.visivel === true, null, { timeout: 7000 }).catch(() => {});
    const p3 = await estado();
    passo('P3 a visita bate (estado_da_porta.pendentes = 1)', p3);
    relatorio.requisitos.R1_faixa_aparece_fora_da_sessao = Boolean(p3.faixa?.visivel);
    await foto('p3-faixa');

    // P4. VER QUEM É.
    // --prova-conserto: a camada da porta fora de #tela-sessao, para provar que o
    // R2 distingue o defeito do conserto (o guarda revertido).
    if (args.includes('--prova-conserto')) {
      await page.evaluate(() => document.body.append(document.getElementById('portaria')));
      relatorio.provaDoConserto = '#portaria movida para o <body> antes do VER QUEM É';
    }
    await page.evaluate(() => document.getElementById('portaria-batendo-ver')?.click());
    await page.waitForTimeout(800);
    const p4 = await estado();
    const pedidoNaTela = await page.evaluate(() => {
      const fila = document.getElementById('portaria-fila');
      return fila ? { texto: fila.innerText.slice(0, 200), visivel: fila.getClientRects().length > 0 && fila.getBoundingClientRect().width > 0 } : null;
    });
    passo('P4 VER QUEM É', { ...p4, pedidoNaTela });
    relatorio.requisitos.R2_janela_da_porta_visivel_com_o_pedido = Boolean(p4.portaria?.visivel && pedidoNaTela?.visivel);
    await foto('p4-ver-quem-e');
    await page.keyboard.press('Escape');

    // P6. Decidir um e chegar outro com a mesma contagem.
    await page.evaluate(() => document.getElementById('portaria-batendo-depois')?.click());
    await page.waitForTimeout(300);
    await page.evaluate((g) => {
      const outro = { ...g.pedidos1[0], impressao: 'ff'.repeat(32), apelido: 'outra-visita', bateu_em: g.pedidos1[0].bateu_em + 60 };
      testTable.pedidos_da_portaria = [outro, { ...g.pedidos1[0], decidido_em: g.pedidos1[0].bateu_em + 30, admitido: true }];
      testTable.estado_da_porta = { ...g.porta1, pendentes: 1 };
    }, GRAVADO);
    await page.waitForTimeout(6500);
    const p6 = await estado();
    passo('P6 DEPOIS; o pedido foi decidido e chegou OUTRO (pendentes continua 1)', p6);
    relatorio.requisitos.R3_pedido_novo_com_a_mesma_contagem_avisa = Boolean(p6.faixa?.visivel);
    await foto('p6-pedido-novo-calado');
  } finally {
    await browser.close();
    server.close();
  }
  relatorio.errosNaPagina = erros;
  const reprovados = Object.entries(relatorio.requisitos).filter(([, ok]) => !ok).map(([r]) => r);
  relatorio.reprovados = reprovados;
  console.log(JSON.stringify(relatorio, null, 2));
  process.exitCode = reprovados.length ? 1 : 0;
})().catch((e) => { console.error(e); process.exitCode = 2; });
