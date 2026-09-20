// Evidência pontual do checkout 7ca66cc. Não é homologação nativa.
// Execute da raiz: node docs/evidencias/revisao-api4-7ca66cc.cjs
// Asserções descrevem os defeitos encontrados; devem ser invertidas ao corrigi-los.
//
// --- nota acrescentada ao corrigir, e não pelo revisor ---
// Os seis defeitos foram consertados, então **este arquivo reprova de
// propósito**: ele afirma o estado defeituoso, que é o que ele existe para
// registrar. A inversão pedida vive em
// `apps/seele-app/bancada/contribuicoes-e-camadas.cjs`, que roda em
// `cargo xtask check-runtime` e mede as mesmas seis coisas contra o
// `index.html` real (e não contra uma hierarquia escrita à mão) e contra o
// roteador real. Cada conserto foi revertido uma vez para ver a bancada
// reprovar pelo nome do defeito.
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../..');
const ler = nome => fs.readFileSync(path.join(root, 'apps/seele-app/ui', nome), 'utf8');
const sessao = { id: 'tela-sessao', inert: false };
const camadas = { id: 'palco-de-camadas', parentNode: sessao, inert: false };
const ctx = vm.createContext({
  console, queueMicrotask: () => {},
  document: { activeElement: null, body: { children: [sessao] } },
  geracaoDaSessao: 7, modsCarregados: new Map(),
  registrarNoAnfitriao() {},
});
vm.runInContext(ler('mods-runtime.js') + '\n' +
  ler('mods-contribuicoes.js') + '\n' + ler('mods-superficies.js') +
  '\nthis.R = RegistroDeContribuicoes; this.S = SuperficieDeMod; this.I = InstanciaDeMod;', ctx);

// O método real opera sobre os filhos do body. A hierarquia acima corresponde
// ao index.html do app; o laboratório põe palco-de-camadas diretamente no body.
ctx.S.prototype.prenderFoco.call({
  palcos: { camadas }, inertes: [],
  raiz: { addEventListener() {} }, focarPrimeiro() {},
});
assert.equal(sessao.inert, true);
console.log('modal: ancestral tela-sessao marcado inert=true');

// Fechar remove o nó, enquanto mostrar apenas troca hidden/dataset.
let removido = false;
const superficie = Object.create(ctx.S.prototype);
Object.assign(superficie, {
  tipo: 'pagina', solta: false, visivel: true, inertes: [],
  raiz: { hidden: false, dataset: {}, remove() { removido = true; } },
});
superficie.fechar();
superficie.mostrar();
assert.equal(removido, true);
assert.equal(superficie.visivel, true);
console.log('fechar -> mostrar: visivel=true, mas nó segue removido');

// Não há distinção entre padrão nativo explícito e ausência de preferência.
const reg = new ctx.R();
const a = new ctx.I('mod/a', 'a', 7, {});
reg.registrar({ id: 'mod/a' }, a, { ponto: 'pessoa.cartao', modo: 'substituir' });
assert.equal(reg.escolherSubstituicao('pessoa.cartao', '', '').escolhida.mod, 'mod/a');
console.log('preferência vazia: seleciona mod/a, não restaura apresentação nativa');

// A cota de contribuições volta a zero; os descartadores continuam retendo objetos.
const repetido = new ctx.R();
const dono = new ctx.I('mod/a', 'a', 7, {});
for (let n = 0; n < 1000; n++) {
  const { handle } = repetido.registrar({ id: 'mod/a' }, dono, {
    ponto: 'canal.item', alvo: '1', conteudo: { texto: 'conteúdo ' + n },
  });
  repetido.revogar(handle);
}
assert.equal(repetido.porHandle.size, 0);
assert.equal(dono.recursos.length, 1000);
console.log('1000 registrar/revogar: contribuições vivas=0; descartadores retidos=1000');

// Exercita o roteador real da janela, sem Tauri, usando instâncias admitidas.
const base = ler('base.js');
const inicio = base.indexOf('async function atenderOMod(');
const fim = base.indexOf('\nconst modsCarregados =', inicio);
assert.ok(inicio >= 0 && fim > inicio);
ctx.contribuicoesDosMods = reg;
vm.runInContext(base.slice(inicio, fim), ctx);
(async () => {
  const respostas = [];
  const b = {
    geracao: 7, admite: () => true, registrar() {},
    executor: { entregar: m => respostas.push(m) },
  };
  ctx.modsCarregados.set('mod/b', b);
  const handleA = [...reg.porHandle.keys()][0];
  await ctx.atenderOMod({ id: 'mod/b', api: 4 }, b, {
    tipo: 'revogar-contribuicao', n: 1, handle: handleA,
  });
  assert.equal(reg.porHandle.has(handleA), false);
  assert.equal(respostas.at(-1).ok, true);
  console.log('mod/b revoga contribuição de mod/a: aceito pelo roteador');

  await ctx.atenderOMod({ id: 'mod/b', api: 3 }, b, {
    tipo: 'contribuir', n: 2,
    pedido: { ponto: 'servidor.navegacao', rotulo: 'API 3' },
  });
  assert.equal(respostas.at(-1).ok, true);
  console.log('mensagem direta API 3 -> contribuir (API 4): aceita pelo roteador');
})().catch(e => { console.error(e); process.exitCode = 1; });

