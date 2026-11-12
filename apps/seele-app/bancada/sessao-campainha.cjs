/* Relógio virtual: percorre o fluxo real de sessão sem esperar 45s na bancada.
 *
 * E a ligação pelo código (ADR 0068): quem atendeu é conferido contra o código
 * ANTES de qualquer escrita no controle, e a recusa acaba a entrada sem escrever.
 * `--reverter-apresentacao` tira a conferência da fonte, e a bancada reprova. */
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const assert = require('node:assert/strict');
let fonte = fs.readFileSync(path.join(__dirname, '../ui/sessao-web.js'), 'utf8');
if (process.argv.includes('--reverter-prazo')) fonte = fonte.replace(
  'await prazo.esperar(Promise.race([sessao.aperto, desistiu]));',
  'await comPrazo(Promise.race([sessao.aperto, desistiu]), PRAZO_DO_APERTO_MS, "HandshakeTimeout");');
if (process.argv.includes('--reverter-apresentacao')) {
  const antes = fonte;
  fonte = fonte.replace('quemAtendeu = conferirQuemAtendeuPeloCodigo(ligacao, wt, alvo);', 'quemAtendeu = null;');
  assert.notEqual(fonte, antes, 'a reversão não achou a conferência da apresentação');
}
const drenar = async () => { for (let i = 0; i < 60; i++) await Promise.resolve(); };
async function caso(tipo) {
  let agora = 0, id = 0, fechar = 0, liberar = 0, ler;
  const timers = new Map();
  const setTimeout = (fn, ms) => { timers.set(++id, { fn, quando: agora + ms }); return id; };
  const clearTimeout = id => timers.delete(id);
  async function avancar(ms) {
    const fim = agora + ms;
    for (;;) {
      const proximo = [...timers].filter(([, t]) => t.quando <= fim).sort((a, b) => a[1].quando - b[1].quando)[0];
      if (!proximo) break;
      agora = proximo[1].quando; timers.delete(proximo[0]); proximo[1].fn(); await drenar();
    }
    agora = fim; await drenar();
  }
  const pendente = { getReader: () => ({ read: () => new Promise(() => {}) }) };
  let passos = [];
  const ligacao = { semente() {}, abrir() {}, bateriaNova() {}, fechar() { liberar++; },
    paraOFio: () => new Uint8Array(), chegou: () => passos, controleAcabou: () => [] };
  const wt = { ready: Promise.resolve(), closed: new Promise(() => {}), close() { fechar++; },
    controle: () => ({ writable: { getWriter: () => ({ write: async () => {}, close: async () => {} }) },
      readable: { getReader: () => ({ read: () => new Promise(r => { ler = r; }) }) } }),
    incomingUnidirectionalStreams: pendente, canalDaVoz: { readable: pendente } };
  const etapas = [], cancelamento = { cancelada: false };
  const contexto = vm.createContext({ console, setTimeout, clearTimeout, setInterval: () => 0, clearInterval() {},
    performance: { now: () => agora }, TextEncoder, TextDecoder, Uint8Array, Uint32Array,
    crypto: { getRandomValues: a => a, subtle: { exportKey: async () => new ArrayBuffer(32) } },
    prepararCertificadoDtls: async () => ({ impressao: 'teste' }), hexDe: () => 'teste', ligacao, wt, cancelamento,
    etapa: e => { if (e.ConnectStageChanged) etapas.push(e.ConnectStageChanged.stage); } });
  vm.runInContext(fonte + '\nfronteiraDaLigacao = () => ligacao;', contexto);
  let resultado;
  vm.runInContext('abrirASessaoDoNavegador({ wasm: {}, alvo: { chave: {}, endereco: "teste" }, cancelamento, emitir: etapa, transporte: () => wt })', contexto)
    .then(s => { resultado = s; }, e => { resultado = e; });
  await drenar();
  async function receber(p) { passos = p; assert.ok(ler); const r = ler; ler = null; r({ value: new Uint8Array([1]), done: false }); await drenar(); }
  await avancar(1_000);
  if (tipo !== 'sem-toque') await receber([{ tocando: { prazo_s: 45 } }]);
  await avancar(11_000);
  if (tipo === 'sem-toque') assert.equal(resultado, 'HandshakeTimeout');
  else {
    assert.equal(resultado, undefined, 'campainha precisa sobreviver ao prazo inicial de 10s');
    assert.ok(etapas.some(e => e.Tocando));
    if (tipo === 'atender') {
      await avancar(20_000); await receber(['entrou']);
      assert.equal(typeof resultado, 'object'); resultado.abortar();
    } else if (tipo === 'cancelar') {
      cancelamento.cancelada = true; cancelamento.aoCancelar(); await drenar();
      assert.equal(resultado, 'Cancelada');
    } else if (tipo === 'recusar') {
      await receber([{ recusada: 'RecusaTeste' }]); assert.equal(resultado, 'RecusaTeste');
    } else {
      // Mesmo um produtor falso de passos não renova o timer da casca.
      await receber([{ tocando: { prazo_s: 45 } }]);
      await avancar(38_999); assert.equal(resultado, undefined);
      await avancar(1); assert.equal(resultado, 'HandshakeTimeout');
      assert.equal(etapas.filter(e => e.Tocando).length, 1);
    }
  }
  assert.equal(fechar, 1); assert.equal(liberar, 1); assert.equal(timers.size, 0);
  console.log(`PASS campainha ${tipo}`);
}
/**
 * A ligação pelo código, com o fluxo real de `abrirASessaoDoNavegador`: a ordem
 * do que acontece no controle é anotada, e a conferência de quem atendeu tem de
 * vir antes da primeira escrita.
 */
async function pelo_codigo(tipo) {
  const CODIGO = 'K9P2-CSM7-NBFR-AYQ3';
  const DTLS = '5a'.repeat(32), CHAVE = 'ab'.repeat(32), PINO = 'cd'.repeat(32);
  const ordem = [], pedidos = [], timers = new Map();
  let id = 0, fechar = 0, liberar = 0, ler, apresentou = false;
  const pendente = { getReader: () => ({ read: () => new Promise(() => {}) }) };
  const ligacao = {
    semente() {}, bateriaNova() {}, fechar() { liberar++; },
    abrir(pedido) { pedidos.push(pedido); return null; },
    // O `.wasm` de verdade só põe o `Hello` na fila depois da apresentação.
    paraOFio: () => (apresentou ? new Uint8Array([7]) : new Uint8Array()),
    apresentada(visto) {
      ordem.push(['apresentada', visto]);
      if (tipo === 'nao-bate') return { recusa: { CodigoNaoBate: { esperado: CODIGO, veio: null } } };
      apresentou = true;
      return { chave: CHAVE };
    },
    chegou: () => [], controleAcabou: () => [],
  };
  const wt = { ready: Promise.resolve(), closed: new Promise(() => {}), close() { fechar++; },
    apresentacao: new Uint8Array(96).fill(1), impressaoDoAnfitriao: DTLS,
    controle: () => ({ writable: { getWriter: () => ({ write: async (bytes) => { ordem.push(['escrita', bytes.length]); }, close: async () => {} }) },
      readable: { getReader: () => ({ read: () => new Promise(r => { ler = r; }) }) } }),
    incomingUnidirectionalStreams: pendente, canalDaVoz: { readable: pendente } };
  const etapas = [], cancelamento = { cancelada: false };
  const alvo = tipo === 'servidor'
    ? { chave: {}, endereco: 'teste', impressao: 'ee'.repeat(32) }
    : { chave: {}, endereco: `@${CODIGO}`, impressao: `@${CODIGO}`, codigo: CODIGO, pino: tipo === 'com-pino' ? PINO : null };
  const contexto = vm.createContext({ console, setTimeout: (fn, ms) => { timers.set(++id, fn); return id; }, clearTimeout: i => timers.delete(i),
    setInterval: () => 0, clearInterval() {}, performance: { now: () => 0 }, TextEncoder, TextDecoder, Uint8Array, Uint32Array,
    crypto: { getRandomValues: a => a, subtle: { exportKey: async () => new ArrayBuffer(32) } },
    prepararCertificadoDtls: async () => ({ impressao: 'teste' }), ligacao, wt, cancelamento, alvo,
    etapa: e => { if (e.ConnectStageChanged) etapas.push(e.ConnectStageChanged.stage); } });
  vm.runInContext(fonte + '\nfronteiraDaLigacao = () => ligacao;', contexto);
  let resultado, recusa;
  vm.runInContext('abrirASessaoDoNavegador({ wasm: {}, alvo, cancelamento, emitir: etapa, transporte: () => wt })', contexto)
    .then(s => { resultado = s; }, e => { recusa = e; });
  await drenar();
  if (tipo === 'servidor') {
    assert.equal(pedidos[0].codigo, undefined, 'a entrada num servidor levou um código ao pedido');
    assert.ok(!ordem.some(([o]) => o === 'apresentada'), 'a entrada num servidor conferiu uma apresentação de sala');
  } else {
    assert.equal(pedidos[0].codigo, CODIGO, 'o pedido não levou o código ao .wasm: o Hello não fica guardado');
    assert.equal(ordem[0]?.[0], 'apresentada', `a primeira coisa no controle não foi conferir quem atendeu: ${JSON.stringify(ordem)}`);
    assert.deepEqual(JSON.parse(JSON.stringify(ordem[0][1])), { apresentacao: '01'.repeat(96), impressao_dtls: DTLS, pino: tipo === 'com-pino' ? PINO : null },
      'a conferência não levou a apresentação, a impressão que o navegador exigiu e o pino');
  }
  if (tipo === 'nao-bate') {
    assert.deepEqual(JSON.parse(JSON.stringify(recusa)), { CodigoNaoBate: { esperado: CODIGO, veio: null } }, 'a recusa de quem não é o dono do código não chegou à casca');
    assert.ok(!ordem.some(([o]) => o === 'escrita'), `algo foi escrito a quem não é o dono do código: ${JSON.stringify(ordem)}`);
    assert.equal(fechar, 1, 'o enlace com quem não é o dono do código ficou aberto');
    assert.equal(liberar, 1);
    assert.ok(etapas.some(e => e.Desistiu), 'a casca não ficou sabendo que a entrada desistiu');
  } else {
    assert.equal(recusa, undefined, `a entrada caiu: ${JSON.stringify(recusa)}`);
    assert.ok(ordem.some(([o]) => o === 'escrita') || tipo === 'servidor', 'depois da conferência o Hello não saiu');
    assert.ok(ler, 'a sessão não começou a ler o controle');
  }
  cancelamento.cancelada = true; cancelamento.aoCancelar?.(); await drenar();
  console.log(`PASS pelo código ${tipo}`);
}
(async () => {
  for (const tipo of ['atender', 'recusar', 'cancelar', 'repetir', 'sem-toque']) await caso(tipo);
  for (const tipo of ['entra', 'com-pino', 'nao-bate', 'servidor']) await pelo_codigo(tipo);
})()
  .catch(e => { console.error(e); process.exitCode = 1; });
