// Reprodução controlada da camada JS; não é teste do transporte nativo.
// Executar na raiz: node docs/evidencias/revisao-mods-fd1de3a.cjs
// Retorna 1 enquanto os comportamentos incorretos forem observados.
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('apps/seele-app/ui/mods-runtime.js', 'utf8');

function bancada() {
  let handler, finishStart;
  let removed = 0;
  const timeouts = [], calls = [];
  const ctx = vm.createContext({
    console,
    setTimeout: (fn) => { timeouts.push(fn); return timeouts.length; },
    listen: async (_name, callback) => {
      handler = callback;
      return () => { removed += 1; };
    },
    invoke: (cmd, args) => {
      calls.push({ cmd, args });
      if (cmd === 'mod_nativo_iniciar') return new Promise((r) => { finishStart = r; });
      return Promise.resolve();
    },
  });
  vm.runInContext(source + '\nglobalThis.api = { executorNativo, InstanciaDeMod };', ctx);
  return {
    api: ctx.api, event: (p) => handler({ payload: p }),
    startDone: (n) => finishStart(n), timeouts, calls,
    removed: () => removed,
  };
}

(async () => {
  const tick = () => new Promise((r) => setImmediate(r));
  const b = bancada(), e = b.api.executorNativo('autor/mod', 7, 'hash');
  let recebidas = 0;
  const init = e.iniciar('', () => { recebidas += 1; e.entregar({ ok: true }); }, () => {});
  await tick();
  b.event({ id: 'autor/mod', geracao: 7, instancia: 99, tipo: 'mensagem', corpo: '{}' });
  b.event({ id: 'autor/mod', geracao: 7, instancia: 99, tipo: 'parou' });
  b.startDone(100);
  await init;
  const fim = e.encerrou();
  await tick();
  b.timeouts.forEach((fn) => fn());
  const falsaConfirmacao = await fim;
  console.log(JSON.stringify({ caso: 'evento antigo durante inicio', recebidas,
    respostasEnviadas: b.calls.filter((c) => c.cmd === 'mod_nativo_entregar').length,
    falsaConfirmacao }));

  const c = bancada(), f = c.api.executorNativo('autor/mod', 8, 'hash');
  const ini = f.iniciar('', () => {}, () => {});
  await tick(); c.startDone(101); await ini;
  const inst = new c.api.InstanciaDeMod('autor/mod', 'hash', 8, f);
  const fechamento = inst.encerrar();
  c.timeouts.forEach((fn) => fn());
  await fechamento;
  c.event({ id: 'autor/mod', geracao: 8, instancia: 101, tipo: 'parou' });
  await tick();
  console.log(JSON.stringify({ caso: 'confirmacao depois do prazo', estado: inst.estado,
    listenersRemovidos: c.removed(), novaChamadaEncerrar: await inst.encerrar() }));
  if (recebidas !== 0 || falsaConfirmacao || inst.estado !== 'encerrada' || c.removed() !== 1) {
    process.exitCode = 1;
  }
})().catch((e) => { console.error(e); process.exitCode = 1; });
