/* Relógio virtual: percorre o fluxo real de sessão sem esperar 45s na bancada. */
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const assert = require('node:assert/strict');
let fonte = fs.readFileSync(path.join(__dirname, '../ui/sessao-web.js'), 'utf8');
if (process.argv.includes('--reverter-prazo')) fonte = fonte.replace(
  'await prazo.esperar(Promise.race([sessao.aperto, desistiu]));',
  'await comPrazo(Promise.race([sessao.aperto, desistiu]), PRAZO_DO_APERTO_MS, "HandshakeTimeout");');
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
(async () => { for (const tipo of ['atender', 'recusar', 'cancelar', 'repetir', 'sem-toque']) await caso(tipo); })()
  .catch(e => { console.error(e); process.exitCode = 1; });
