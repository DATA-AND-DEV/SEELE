// Sessão real com escritor controlado: nenhum prazo de produção é aguardado.
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const path = require('node:path');
const defer = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const limite = p => Promise.race([p, sleep(300).then(() => { throw Error('flush impediu o desfecho'); })]);
async function caso(tipo) {
  const gate = defer(), leitura = defer(), pronta = defer();
  let writes = 0, closes = 0, fechadas = 0, liberadas = 0;
  const motivo = { Refused: { reason: 'AdmissionPending' } };
  const escritor = { write() { writes++; pronta.resolve(); return gate.promise; }, close() { closes++; return tipo === 'close-pendente' ? new Promise(() => {}) : Promise.resolve(); } };
  let bytes = true;
  const ligacao = { semente() {}, abrir() {}, bateriaNova() {}, fechar() { liberadas++; },
    paraOFio() { const b = bytes; bytes = false; return new Uint8Array(b ? [1] : []); },
    chegou() { bytes = true; return [{ recusada: motivo }]; }, controleAcabou() { return []; } };
  const pendente = { getReader: () => ({ read: () => new Promise(() => {}) }) };
  let leu = false;
  const wt = { ready: Promise.resolve(), closed: new Promise(() => {}), close() { fechadas++; },
    controle: () => ({ writable: { getWriter: () => escritor }, readable: { getReader: () => ({ read: () => { if (leu) return new Promise(() => {}); leu = true; return leitura.promise; } }) } }),
    incomingUnidirectionalStreams: pendente, canalDaVoz: { readable: pendente } };
  const contexto = vm.createContext({ console, setTimeout, clearTimeout, clearInterval, performance, TextEncoder, TextDecoder, Uint8Array, Uint32Array,
    crypto: { getRandomValues: a => a, subtle: { exportKey: async () => new ArrayBuffer(32) } },
    prepararCertificadoDtls: async () => ({ impressao: 'teste' }), hexDe: () => 'teste', ligacao, wt });
  let fonte = fs.readFileSync(path.join(__dirname, '../ui/sessao-web.js'), 'utf8')
    .replace('PRAZO_DO_APERTO_MS = 10_000', 'PRAZO_DO_APERTO_MS = 60')
    .replace('ESPERA_DA_RECUSA_MS = 1_000', 'ESPERA_DA_RECUSA_MS = 25');
  if (process.argv.includes('--reverter-flush')) fonte = fonte.replace(
    'const deste = escritor;\n        try {',
    'const deste = escritor;\n        await escreverOQueEspera();\n        try {');
  vm.runInContext(fonte + '\nfronteiraDaLigacao = () => ligacao;', contexto);
  const cancelamento = { cancelada: false };
  contexto.cancelamento = cancelamento;
  if (tipo === 'encerrar' || tipo === 'close-pendente') {
    const sessao = vm.runInContext('criarASessaoAberta({ ligacao, wt, alvo: {}, pedido: {}, emitir() {}, aoAcabar() {} })', contexto);
    await pronta.promise;
    if (tipo === 'close-pendente') gate.resolve();
    await limite(sessao.encerrar());
    assert.equal(sessao.acabou, true);
  } else {
    const entrada = vm.runInContext('abrirASessaoDoNavegador({ wasm: {}, alvo: { chave: {}, endereco: "teste" }, cancelamento, transporte: () => wt })', contexto);
    const resultado = entrada.then(() => { throw Error('entrada indevida'); }, e => e);
    await pronta.promise;
    if (tipo === 'cancelar') { cancelamento.cancelada = true; cancelamento.aoCancelar(); }
    if (tipo.startsWith('recusa')) leitura.resolve({ value: new Uint8Array([2]), done: false });
    if (tipo === 'recusa-entregue') gate.resolve();
    const recebido = await limite(resultado);
    assert.deepEqual(recebido, tipo === 'cancelar' ? 'Cancelada' : tipo.startsWith('recusa') ? motivo : 'HandshakeTimeout');
  }
  assert.equal(fechadas, 1, 'transporte deve fechar uma vez');
  assert.equal(liberadas, 1, 'ligação deve ser liberada uma vez');
  gate.resolve(); await sleep(0);
  assert.equal(writes, tipo === 'recusa-entregue' ? 2 : 1, 'fila tardia não pode escrever depois de fechar');
  assert.equal(closes, ['close-pendente', 'recusa-entregue'].includes(tipo) ? 1 : 0, 'flush tardio não pode fechar escritor depois do transporte');
  console.log(`PASS ${tipo}`);
}
(async () => { for (const tipo of ['encerrar', 'cancelar', 'prazo', 'recusa', 'close-pendente', 'recusa-entregue']) await caso(tipo); })().catch(e => { console.error(e); process.exitCode = 1; });
