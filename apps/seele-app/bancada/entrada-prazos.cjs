// UI, identidade, ponte e WASM reais; APIs locais atrasadas sob controle.
// Os prazos só são encurtados no fonte SERVIDO por esta bancada, nunca no produto.
// node entrada-prazos.cjs [--webkit] [--playwright=...] [--reverter-cancelamento]
const fs = require('node:fs');
const path = require('node:path');
const pw = require('./playwright.cjs');
const {origemDoProduto} = require('./origem-do-produto.cjs');
(async () => {
  const browser = await (process.argv.includes('--webkit') ? pw.webkit() : pw.chromium()).launch();
  try {
    const page = await browser.newPage({ viewport: { width: 440, height: 956 }, isMobile: true, hasTouch: true });
    await page.route(`${origemDoProduto}/**`, route => {
      const nome = new URL(route.request().url()).pathname.slice(1) || 'index.html';
      const arquivo = nome === 'seele_web.wasm' ? path.resolve(__dirname, '../../../target/wasm32-unknown-unknown/release/seele_web.wasm') : path.resolve(__dirname, '../ui', nome);
      if (!fs.existsSync(arquivo)) return route.fulfill({ status: 404, body: '' });
      let body = fs.readFileSync(arquivo);
      if (nome.endsWith('.js')) {
        body = body.toString().replace('PRAZO_DA_PREPARACAO_DA_ENTRADA_MS = 30_000', 'PRAZO_DA_PREPARACAO_DA_ENTRADA_MS = 200')
          .replace('PRAZO_DO_BANCO_DA_IDENTIDADE_MS = 3_000', 'PRAZO_DO_BANCO_DA_IDENTIDADE_MS = 80');
        if (process.argv.includes('--reverter-cancelamento') && nome === 'ponte-web.js') {
          body = body.replace('entradaEmCurso.cancelar = cancelar;', '')
            .replace('sessao = await abrirASessaoDoNavegador({', 'entradaEmCurso.cancelar = cancelar;\n    sessao = await abrirASessaoDoNavegador({');
        }
      }
      const tipos = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.wasm': 'application/wasm' };
      return route.fulfill({ contentType: tipos[path.extname(nome)] || 'application/octet-stream', body });
    });
    await page.goto(`${origemDoProduto}/`);
    const casos = await page.evaluate(async () => {
      await identidadeDaPagina();
      const resultados = [];
      const confere = (ok, motivo) => { if (!ok) throw new Error(motivo); };
      const esperar = ms => new Promise(r => setTimeout(r, ms));
      const limite = p => Promise.race([p, esperar(1500).then(() => { throw new Error('operação não terminou'); })]);
      const defer = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
      const gerar = crypto.subtle.generateKey.bind(crypto.subtle);
      const chave = await gerar({ name: 'Ed25519' }, false, ['sign', 'verify']);
      const storage = navigator.storage;
      Object.defineProperty(navigator, 'storage', { value: { persist: async () => false }, configurable: true });
      let serial = 0;
      const criar = (conectar, soltar = () => {}) => criarIdentidadeDoNavegador({
        analisar: () => ({ link: { alvo: '192.0.2.1:8383', impressao: `bancada-${++serial}`, versao: '2.0', convite: 'credencial-da-bancada' } }), conectar, soltar,
      });
      // Cancelar resolve mesmo com API pendente; liberar depois não abre nada.
      for (const etapa of ['persist', 'chave', 'guardar-sucesso', 'guardar-admissao', 'conexao-tardia']) {
        const gate = defer(), iniciou = defer();
        const banco = noBanco;
        let conexoes = 0, soltas = 0;
        if (etapa === 'persist') navigator.storage.persist = () => { iniciou.resolve(); return gate.promise; };
        if (etapa === 'chave') crypto.subtle.generateKey = () => { iniciou.resolve(); return gate.promise; };
        if (etapa.startsWith('guardar')) noBanco = (modo, fazer, opts) => {
          if (modo !== 'readwrite') return banco(modo, fazer, opts);
          iniciou.resolve(); return gate.promise;
        };
        const identidade = criar(async () => {
          conexoes++;
          if (etapa === 'guardar-admissao') throw { Refused: { reason: 'AdmissionPending' } };
          if (etapa === 'conexao-tardia') { iniciou.resolve(); return gate.promise; }
          return { geracao: 100 };
        }, () => { soltas++; });
        await identidade.abrir('teste');
        const entrada = identidade.entrar('bancada');
        await limite(iniciou.promise);
        identidade.agoraNao();
        const resposta = await limite(entrada);
        confere(resposta.erro === 'sem-tentativa', `${etapa}: cancelamento não terminou a espera`);
        await esperar(0);
        confere(soltas === (etapa === 'guardar-sucesso' ? 1 : 0), `${etapa}: sessão não foi solta antes de liberar banco`);
        gate.resolve(etapa === 'chave' ? chave : etapa === 'conexao-tardia' ? { geracao: 101 } : false);
        await esperar(0);
        confere(conexoes === (['persist', 'chave'].includes(etapa) ? 0 : 1), `${etapa}: abriu conexão tardia`);
        confere(soltas === (['guardar-sucesso', 'conexao-tardia'].includes(etapa) ? 1 : 0), `${etapa}: recurso tardio não foi fechado uma vez`);
        navigator.storage.persist = async () => false; crypto.subtle.generateKey = gerar; noBanco = banco;
        resultados.push(`cancelamento ${etapa}`);
      }
      // Preparação tem UM orçamento: persist consumiu parte dele antes da chave.
      {
        const gate = defer(); let conexoes = 0;
        navigator.storage.persist = async () => { await esperar(120); return false; };
        crypto.subtle.generateKey = () => gate.promise;
        const identidade = criar(async () => { conexoes++; return { geracao: 102 }; });
        await identidade.abrir('teste');
        const inicio = performance.now();
        const resposta = await limite(identidade.entrar('bancada'));
        const duracao = performance.now() - inicio;
        confere(resposta.erro === 'preparacao-expirada', 'prazo local virou falha de rede');
        confere(duracao < 290, `orçamento reiniciou entre etapas: ${duracao}ms`);
        gate.resolve(chave); await esperar(0);
        confere(conexoes === 0 && identidade.ler() !== null, 'prazo abriu conexão ou descartou tentativa');
        navigator.storage.persist = async () => false; crypto.subtle.generateKey = gerar;
        resultados.push('prazo único da preparação e retorno tardio descartado');
      }
      // Falha de gravação fecha a sessão, retém chave e permite repetir sem token.
      for (const admissao of [false, true]) {
        const gate = defer(), banco = noBanco;
        noBanco = (modo, fazer, opts) => modo === 'readwrite' ? gate.promise : banco(modo, fazer, opts);
        let soltas = 0; const chamadas = [];
        const identidade = criar(async (alvo, apelido, token) => {
          chamadas.push({ chave: alvo.chave, token, conhecido: alvo.conhecido });
          if (admissao && chamadas.length === 1) throw { Refused: { reason: 'AdmissionPending' } };
          return { geracao: 103 };
        }, () => { soltas++; });
        await identidade.abrir('teste');
        const resposta = await limite(identidade.entrar('bancada'));
        confere(resposta.erro === 'armazenamento-indisponivel', 'banco pendente não terminou com motivo próprio');
        await esperar(0);
        confere(soltas === (admissao ? 0 : 1), 'sessão não foi fechada no prazo do banco');
        gate.resolve(false); noBanco = banco;
        const repetiu = await limite(identidade.entrar('bancada'));
        confere(repetiu.ok && chamadas.length === 2 && chamadas[0].chave === chamadas[1].chave, 'retry perdeu a chave');
        if (!admissao) confere(chamadas[1].token === null && chamadas[1].conhecido, 'retry de sessão aceita reenviou convite');
        resultados.push(admissao ? 'prazo da gravação da portaria' : 'gravação falhou fechado; retry pela mesma chave');
      }
      // Cancelar uma entrada velha não limpa o cancelamento da entrada nova.
      {
        const instanciar = instanciarOWasmDaSessao, abrir = abrirASessaoDoNavegador;
        const gates = [defer(), defer()]; let n = 0, abriu = 0;
        instanciarOWasmDaSessao = () => gates[n++].promise;
        abrirASessaoDoNavegador = async () => { abriu++; throw 'não deve abrir'; };
        const alvo = { endereco: '192.0.2.1:8383', impressao: 'a'.repeat(64), chave };
        const primeira = abrirASessaoPelaPonte({ alvo }).catch(e => e);
        await esperar(0); const cancelarPrimeira = entradaEmCurso.cancelar;
        confere(typeof cancelarPrimeira === 'function', 'WASM começou sem cancelamento');
        const segunda = abrirASessaoPelaPonte({ alvo: { ...alvo, impressao: 'b'.repeat(64) } }).catch(e => e);
        await esperar(0); const cancelarSegunda = entradaEmCurso.cancelar;
        cancelarPrimeira();
        confere(await limite(primeira) === 'Cancelada', 'cancelamento não encerrou preparação WASM');
        confere(entradaEmCurso.cancelar === cancelarSegunda, 'finally antigo apagou cancelamento novo');
        confere(ATENDIMENTOS_DA_PONTE.cancelar_ligacao() === true, 'segunda entrada não pode cancelar');
        await limite(segunda); gates.forEach(g => g.resolve({})); await esperar(0);
        confere(abriu === 0, 'WASM tardio abriu sessão');
        instanciarOWasmDaSessao = instanciar; abrirASessaoDoNavegador = abrir;
        resultados.push('cancelamento antes do WASM e duas entradas concorrentes');
      }
      // Certificado/exportKey: o prazo restante passa da ponte até a sessão.
      for (const etapa of ['certificado', 'exportar']) {
        const prep = prepararCertificadoDtls, exp = crypto.subtle.exportKey.bind(crypto.subtle);
        const gate = defer(), inicio = defer(); let transportes = 0;
        const preparado = await prep();
        const wasm = await instanciarOWasmDaSessao();
        if (etapa === 'certificado') prepararCertificadoDtls = () => { inicio.resolve(); return gate.promise; };
        else crypto.subtle.exportKey = (...args) => args[0] === 'raw' ? (inicio.resolve(), gate.promise) : exp(...args);
        const ctrl = new AbortController();
        const entrada = abrirASessaoDoNavegador({ wasm, alvo: { endereco: '192.0.2.1:8383', impressao: 'a'.repeat(64), chave }, apelido: 'bancada', preparacao: { sinal: ctrl.signal, ate: Date.now() + 200 }, transporte: () => { transportes++; throw 'não deve abrir'; } }).catch(e => e);
        await limite(inicio.promise); ctrl.abort('Cancelada');
        confere(await limite(entrada) === 'Cancelada', `${etapa}: cancelamento esperou API`);
        gate.resolve(etapa === 'certificado' ? preparado : await exp('raw', chave.publicKey));
        await esperar(0); confere(transportes === 0, `${etapa}: abriu transporte tardio`);
        prepararCertificadoDtls = prep; crypto.subtle.exportKey = exp;
        resultados.push(`cancelamento ${etapa} antes do RTC`);
      }
      // O orçamento da ponte NÃO reinicia ao chegar ao certificado na sessão.
      {
        const instanciar = instanciarOWasmDaSessao, prep = prepararCertificadoDtls, transporte = abrirOEnlace;
        const gate = defer(); let abriu = 0;
        const pronto = await prep();
        instanciarOWasmDaSessao = async () => { await esperar(120); return instanciar(); };
        prepararCertificadoDtls = () => gate.promise;
        abrirOEnlace = () => { abriu++; throw 'não deve abrir'; };
        const inicio = performance.now();
        const resposta = await limite(abrirASessaoPelaPonte({ alvo: { endereco: '192.0.2.1:8383', impressao: 'c'.repeat(64), chave }, nickname: 'bancada' }).catch(e => e));
        const duracao = performance.now() - inicio;
        confere(resposta === 'PreparacaoExpirada' && duracao < 290, `ponte/sessão reiniciou prazo: ${resposta}, ${duracao}ms`);
        gate.resolve(pronto); await esperar(0); confere(abriu === 0, 'certificado tardio abriu RTC');
        instanciarOWasmDaSessao = instanciar; prepararCertificadoDtls = prep; abrirOEnlace = transporte;
        resultados.push('prazo compartilhado WASM/certificado; RTC não aberto depois');
      }
      // A tela real sai da animação com motivo local e mantém ENTRAR para retry.
      for (const etapa of ['preparacao', 'armazenamento']) {
        const banco = noBanco; const gate = defer(); let soltas = 0;
        if (etapa === 'preparacao') navigator.storage.persist = () => gate.promise;
        else noBanco = (modo, fazer, opts) => modo === 'readwrite' ? gate.promise : banco(modo, fazer, opts);
        const identidade = criar(async () => ({ geracao: 105 }), () => { soltas++; });
        await CELULAR.ajudante('entrada').usar(identidade, { link: 'teste' });
        document.getElementById('cel-entrada-apelido').value = 'bancada UI';
        [...document.querySelectorAll('.cel-entrada-pe button')].find(b => b.textContent === 'ENTRAR').click();
        await esperar(250);
        const texto = document.querySelector('.cel-entrada').textContent;
        confere(texto.includes(etapa === 'preparacao' ? 'O NAVEGADOR NÃO CONCLUIU A PREPARAÇÃO' : 'O NAVEGADOR NÃO GUARDOU A IDENTIDADE'), `${etapa}: tela não explicou motivo local`);
        confere(!document.querySelector('.cel-entrada-conectando'), `${etapa}: animação continuou`);
        confere([...document.querySelectorAll('.cel-entrada-pe button')].some(b => b.textContent === 'ENTRAR' && !b.disabled), `${etapa}: retry sumiu`);
        confere(soltas === (etapa === 'armazenamento' ? 1 : 0), `${etapa}: recurso não fechou`);
        gate.resolve(false); noBanco = banco; navigator.storage.persist = async () => false;
        resultados.push(`UI: ${etapa} termina e oferece retry`);
      }
      // IndexedDB: abertura tardia é fechada; transação pendente é abortada.
      {
        const original = indexedDB.open.bind(indexedDB);
        let pedido, fechou = 0, transacoes = 0, abortou = 0;
        indexedDB.open = () => (pedido = {});
        const ctrl = new AbortController();
        const operacao = noBanco('readwrite', () => ({ result: null }), { sinal: ctrl.signal }).catch(e => e);
        ctrl.abort('Cancelada'); await limite(operacao);
        pedido.result = { close: () => fechou++, transaction: () => { transacoes++; } };
        pedido.onsuccess();
        confere(fechou === 1 && transacoes === 0, 'abertura tardia do banco não fechou');
        const gravando = noBanco('readwrite', () => ({ result: null })).catch(e => e);
        pedido.result = { close: () => fechou++, transaction: () => { transacoes++; return { abort: () => abortou++, objectStore: () => ({}) }; } };
        pedido.onsuccess();
        confere(await limite(gravando) === 'ArmazenamentoIndisponivel', 'transação não venceu');
        confere(abortou === 1 && fechou === 2, 'banco ficou ativo depois do prazo');
        indexedDB.open = original;
        resultados.push('IndexedDB abortado e abertura tardia fechada');
      }
      // Banco bloqueado não transforma identidade existente em pessoa nova.
      {
        const banco = noBanco; let novas = 0, conexoes = 0, chaveUsada = null;
        const registro = { impressao: 'leitura-bloqueada', endereco: '192.0.2.5:8383', chave, versao: '2.0', apelido: 'bancada', visto: Date.now() };
        await guardarOServidor(registro);
        noBanco = (modo, fazer, opts) => modo === 'readonly' ? Promise.reject('ArmazenamentoIndisponivel') : banco(modo, fazer, opts);
        crypto.subtle.generateKey = (...args) => { novas++; return gerar(...args); };
        const identidade = criarIdentidadeDoNavegador({ analisar: () => ({ link: { alvo: registro.endereco, impressao: registro.impressao, versao: '2.0' } }), conectar: async alvo => { conexoes++; chaveUsada = alvo.chave; return { geracao: 106 }; } });
        await CELULAR.ajudante('entrada').usar(identidade, { link: 'teste' });
        confere(identidade.ler().estado === 'armazenamento-indisponivel', 'leitura falha virou desconhecido');
        confere((await identidade.entrarNoConhecido(registro.impressao, 'bancada')).erro === 'armazenamento-indisponivel', 'lista confundiu falha de leitura com servidor desconhecido');

        confere((await identidade.entrar('bancada')).erro === 'armazenamento-indisponivel' && novas === 0 && conexoes === 0, 'leitura falha criou chave/conectou');
        confere(document.querySelector('.cel-entrada').textContent.includes('TENTAR LER DE NOVO'), 'UI não ofereceu repetir leitura');
        noBanco = banco;
        await identidade.reler();
        confere(identidade.ler().estado === 'ja-conhecido', 'leitura recuperada não reconheceu identidade');
        confere((await identidade.entrar('bancada')).ok && novas === 0 && conexoes === 1, 'retry não usou identidade guardada');
        const original = new Uint8Array(await crypto.subtle.exportKey('raw', chave.publicKey));
        const usada = new Uint8Array(await crypto.subtle.exportKey('raw', chaveUsada.publicKey));
        confere(original.every((b, i) => b === usada[i]), 'retry usou outra chave');
        crypto.subtle.generateKey = gerar;
        resultados.push('leitura falha fecha entrada; retry recupera chave original');
      }
      // Resposta antiga do mesmo servidor não sobrescreve chave nova já guardada.
      {
        const gate = defer(), chegou = defer(), guardouTardia = defer();
        const guardarTardia = guardarOServidorSeAusente;
        guardarOServidorSeAusente = async registro => { try { return await guardarTardia(registro); } finally { guardouTardia.resolve(); } };
        const chamadas = [], soltas = [];
        const identidade = criarIdentidadeDoNavegador({
          analisar: () => ({ link: { alvo: '192.0.2.6:8383', impressao: 'mesma-impressao-tardia', versao: '2.0' } }),
          conectar: async alvo => { chamadas.push(alvo.chave); if (chamadas.length === 1) { chegou.resolve(); return gate.promise; } return { geracao: 202 }; },
          soltar: resposta => soltas.push(resposta.geracao),
        });
        await identidade.abrir('primeiro'); const primeira = identidade.entrar('bancada');
        await limite(chegou.promise); await identidade.abrir('segundo');
        confere((await primeira).erro === 'sem-tentativa', 'primeira não cancelou');
        confere((await identidade.entrar('bancada')).ok, 'segunda não entrou');
        gate.resolve({ geracao: 201 }); await limite(guardouTardia.promise);
        const registro = await servidorConhecido('mesma-impressao-tardia');
        const novo = new Uint8Array(await crypto.subtle.exportKey('raw', chamadas[1].publicKey));
        const guardado = new Uint8Array(await crypto.subtle.exportKey('raw', registro.chave.publicKey));
        confere(novo.every((b, i) => b === guardado[i]), 'aceite tardio substituiu chave nova');
        confere(soltas.length === 1 && soltas[0] === 201, 'fecho tardio atingiu nova sessão');
        guardarOServidorSeAusente = guardarTardia;
        resultados.push('aceite tardio preserva atomicamente registro novo do mesmo servidor');
      }
      // O apelido não existe nos estados bloqueados; Enter não deve lançar.
      for (const estado of ['incompleto', 'vencido']) {
        await CELULAR.ajudante('entrada').usar({ ler: () => ({ estado, valeAte: 1 }), agoraNao: () => {} });
        confere(!document.getElementById('cel-entrada-apelido'), `${estado}: apareceu campo de entrada`);
        confere(![...document.querySelectorAll('.cel-entrada-pe button')].some(b => b.textContent === 'ENTRAR'), `${estado}: ofereceu conexão`);
        resultados.push(`UI bloqueada ${estado} sem erro de teclado`);
      }
      // Soltar a entrada por geração preserva a outra sessão, vista e foco de voz.
      {
        let antiga = 0, nova = 0;
        const uma = { idDaPonte: 401, geracao: 401, abortar: () => antiga++, encerrar: async () => {}, alvo: {} };
        const outra = { idDaPonte: 402, geracao: 402, abortar: () => nova++, encerrar: async () => {}, alvo: {} };
        sessoesDaPonte.set(401, uma); sessoesDaPonte.set(402, outra);
        vistaDaPonte = 402; focoDaPonte = 402;
        await ATENDIMENTOS_DA_PONTE.soltar_a_entrada({ geracao: 401 });
        confere(antiga === 1 && nova === 0 && sessoesDaPonte.has(402) && vistaDaPonte === 402 && focoDaPonte === 402, 'fecho antigo afetou sessão nova/voz');
        await ATENDIMENTOS_DA_PONTE.disconnect({});
        confere(!sessoesDaPonte.has(402) && vistaDaPonte === 0 && focoDaPonte === 0, 'envelope do disconnect foi confundido com sessão');
        resultados.push('fecho por geração preserva outra sessão e voz');
        resultados.push('disconnect ignora envelope e fecha a sessão atual');
      }
      Object.defineProperty(navigator, 'storage', { value: storage, configurable: true });
      return resultados;
    });
    for (const caso of casos) console.log(`OK: ${caso}`);
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
