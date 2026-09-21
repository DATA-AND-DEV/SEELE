// A fila usada pelo produto, com o balde de controle do servidor e relógio virtual.
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const fonte = fs.readFileSync(require('node:path').join(__dirname, '../ui/base.js'), 'utf8');
const trecho = fonte.slice(fonte.indexOf('let filaDePedidosDeMod ='), fonte.indexOf('/**\n * Põe um MOD de pé'));
let now = 0, last = 0, tokens = 60, other = 0, generation = 1, enviados = 0;
function frame(t) {
  tokens = Math.min(60, tokens + (t - last) * .02); last = t;
  assert.ok(tokens >= 1, 'o servidor descartaria este pedido'); tokens--;
}
const pendentes = new Map();
const contexto = vm.createContext({
  geracaoDaSessao: 1, ouvirMod: Promise.resolve(), TextEncoder,
  daGeracaoDePe: n => n === generation,
  pedidosDeMod: pendentes, proximoPedidoDeMod: 0,
  registrarNoAnfitriao() {}, clearTimeout() {},
  setTimeout(fn, ms) {
    if (ms === 15000) return 1; // Prazo de resposta, não a cadência.
    now += ms; queueMicrotask(fn); return 1;
  },
  invoke: async (_, args) => {
    // Dez quadros/s de voz/controle/outros recursos, além dos MODs.
    while (other <= now) { frame(other); other += 100; }
    frame(now); enviados++;
    const pedido = pendentes.get(args.request); pendentes.delete(args.request);
    pedido.resolve({ ok: true });
  },
});
vm.runInContext(trecho, contexto);
(async () => {
  const partes = Math.ceil((4 * Math.ceil(10 * 1024 * 1024 / 3) + 22) / 6000);
  const upload = async id => { for (let i = 0; i < partes; i++) await contexto.pedirAoServidor(id, 1, { op: 'upload-part', index: i }); };
  await Promise.all([upload('seele/perfis'), upload('seele/mesa')]);
  assert.equal(enviados, partes * 2);
  const antes = enviados;
  const antigo = contexto.pedirAoServidor('seele/perfis', 1, {});
  generation++;
  await assert.rejects(antigo, /disconnected/);
  assert.equal(enviados, antes, 'pedido da sessão anterior atravessou');
  console.log('imagens: dois uploads de 10 MiB e tráfego concorrente cabem no balde; saída cancela pedidos antigos');
})().catch(e => { console.error(e); process.exitCode = 1; });
