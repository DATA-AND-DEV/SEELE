// Revisão complementar de 26ad0c2. Usa árvore e classes da bancada do produto.
// Rendering simulado: prova transições/registro de recursos, não homologação visual.
// Execute: node docs/evidencias/revisao-api4-26ad0c2.cjs
//
// --- nota acrescentada ao corrigir, e não pelo revisor ---
// Os três grupos foram consertados, então **este arquivo reprova de
// propósito** — e o primeiro caso reprova por uma razão a mais: `this.inertes`
// deixou de existir. A inércia passou a ser decidida por uma pilha de camadas
// (`pilhaDeCamadas` em `mods-superficies.js`), porque preservar o booleano
// anterior de cada nó nunca ia resolver dois donos simultâneos querendo o
// mesmo nó adormecido — que é o que a seção 1 desta revisão diz.
//
// As três medidas estão em `apps/seele-app/bancada/contribuicoes-e-camadas.cjs`,
// nas seções R1c (reabrir), R1d (ordem inversa), R4b (revogar com desenho
// montado), R4c (mil superfícies) e R3b (cartão genérico, por destino). Cada
// conserto foi revertido uma vez para ver a bancada reprovar com os mesmos
// números desta reprodução: 1000 retidos, 0 chamadas de `soltar`, o mesmo nó
// para duas pessoas.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../..');
const bancadaDir = path.join(root, 'apps/seele-app/bancada');
const codigo = fs.readFileSync(path.join(bancadaDir, 'contribuicoes-e-camadas.cjs'), 'utf8');
const marco = '// ------------------------------------------------------- R1 · o ramo do modal';
assert.ok(codigo.includes(marco));
const env = new Function('require', '__dirname', codigo.slice(0, codigo.indexOf(marco)) +
  '\nreturn { contexto, No, camadas, sessao, documento, S, I, R };'
)(require, bancadaDir);
const {contexto: ctx, No, camadas, sessao, S, I, R} = env;
const ler = nome => fs.readFileSync(path.join(root, 'apps/seele-app/ui', nome), 'utf8');
const criarDialogo = () => {
  const camada = new No('div');
  camadas.append(camada);
  return {
    camada, palcos: {camadas}, inertes: [], solta: false,
    raiz: new No('div'), focarPrimeiro() {},
  };
};
const limparInercia = no => { no.inert = false; for(const f of no.children) limparInercia(f); };
const fundo = sessao.children.find(n => n !== camadas);

// mostrar -> abrir chama prenderFoco. Reutilizar via criar ainda chama abrir outra vez.
const a = criarDialogo();
S.prototype.prenderFoco.call(a);
const primeiraLista = a.inertes.length;
S.prototype.prenderFoco.call(a);
const segundaLista = a.inertes.length;
S.prototype.soltarFoco.call(a);
assert.ok(primeiraLista > 0);
assert.equal(segundaLista, 0);
assert.equal(fundo.inert, true);
assert.equal(a.raiz.ouvintes, 1);
console.log('prender foco duas vezes -> fechar: fundo inerte=true; ouvinte restante=1');
a.camada.remove();
limparInercia(env.documento.body);

// Camadas em ordem inversa: B não reivindica os irmãos que A já tornou inertes.
const b1 = criarDialogo();
S.prototype.prenderFoco.call(b1);
const b2 = criarDialogo();
S.prototype.prenderFoco.call(b2);
S.prototype.soltarFoco.call(b1);
assert.equal(fundo.inert, false);
console.log('abrir A, abrir B, fechar A: fundo inerte=false com B ainda aberto');
S.prototype.soltarFoco.call(b2);
b1.camada.remove(); b2.camada.remove();
limparInercia(env.documento.body);

// Montagem usa o renderer simulado, mas registrar/montar/revogar são os reais.
let descartes = 0;
ctx.RegiaoDeMod = class {
  aplicar() { return 0; }
  soltar() { descartes++; }
};
ctx.PERFIS_DE_RENDER = {cartao: {}, superficie: {}};
ctx.elemento = tag => new No(tag);
ctx.donoDaRegiao = (mod, instancia) => ({instancia, falar() {}});
const base = ler('base.js');
const inicio = base.indexOf('function montarContribuicao(');
const fim = base.indexOf('\n/**\n * Os nós de todas as contribuições', inicio);
assert.ok(inicio >= 0 && fim > inicio);
vm.runInContext(base.slice(inicio, fim), ctx);
const inst = new I('mod/a', 'hash', 7, {});
inst.estado = 'ativa';
const reg = new R();
const {handle} = reg.registrar({id:'mod/a'}, inst, {
  ponto:'pessoa.cartao', modo:'adicionar', conteudo:{forma:'texto',texto:'Teste'},
});
const c = reg.porHandle.get(handle);
ctx.montarContribuicao(c);
assert.equal(inst.recursos.length, 2);
reg.revogar(handle, {id:'mod/a', instancia:inst});
assert.equal(inst.recursos.length, 1);
assert.equal(descartes, 0);
console.log('revogar contribuição montada: recursos retidos=1; renderer.soltar chamado=0');

// Superfícies: só a casca e o renderer são simulados; constructor/descartar reais.
S.prototype.montarCasca = function() {
  this.raiz = new No('div');
  this.corpo = new No('div');
};
const si = new I('mod/s', 'hash', 7, {});
for(let i=0;i<1000;i++) {
  const superficie = new S('mod/s', {instancia:si}, {id:String(i),tipo:'pagina'}, {});
  superficie.descartar();
}
assert.equal(si.recursos.length, 1000);
console.log('1000 superfícies criar/descartar: recursos retidos=1000');

