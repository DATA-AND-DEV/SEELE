// Medição de serialização, sem navegador e sem alterar o produto.
// Usa o prelúdio, o cliente e o servidor reais; simula respostas do anfitrião.
// Complementa a falha observada no aplicativo nativo, não o substitui.
//
// --- nota acrescentada ao corrigir, e não pelo revisor ---
// **Este arquivo reprova de propósito**, e por dois motivos.
//
// O primeiro é o conserto: o `superficie-montar` do ESTILO foi de 14.164 para
// 4.853 bytes — as três abas eram montadas de uma vez, e só a aberta vai
// agora. O segundo é N2: ele espera uma mensagem `regiao` na carga inicial, e
// um pacote de API 4 não manda nenhuma — a faixa permanente saiu.
//
// A medida virou guarda em `test/cliente-api3.test.cjs` dos três pacotes
// (`cabeNaPonte`), onde ela roda a cada `npm test` e vale para toda mensagem
// que atravessa a ponte. Foi ela que encontrou um segundo caso que esta
// rodada não viu: o diretório do PERFIS com setenta pessoas dava 52.862
// bytes, e no servidor de teste havia uma pessoa só.
//
// O prelúdio passou a separar as duas recusas: uma mensagem grande demais diz
// `mensagem-grande` com o tamanho e o teto, em vez de `fila-cheia`.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');

const produto = path.resolve(__dirname, '../..');
const mod = path.resolve(produto, '../SEELE-MOD-ESTILO');
const executor = fs.readFileSync(path.join(produto, 'apps/seele-app/src/executor.rs'), 'utf8');
const preludio = executor.split('const PRELUDIO: &str = r#"')[1]?.split('"#;')[0];
assert.ok(preludio, 'prelúdio do produto não encontrado');
const servidor = vm.createContext({
  dados: {}, mundo: { agora: () => 100 },
  arquivos: { ler: () => null, escrever: () => true, listar: () => [] },
});
vm.runInContext(fs.readFileSync(path.join(mod, 'servidor/main.js'), 'utf8'), servidor);
const medidas = [];
let cliente;
cliente = vm.createContext({
  console,
  __seeleCapacidades: ['superficies', 'contribuicoes'],
  seele: {
    postar(texto) {
      const m = JSON.parse(texto);
      const bytes = Buffer.byteLength(texto, 'utf8');
      medidas.push({ tipo: m.tipo, bytes });
      // O limite individual observado em executor.rs neste checkout.
      if (bytes > 12 * 1024) return false;
      setImmediate(() => {
        let valor = {};
        if (m.tipo === 'snapshot') valor = {
          me: 1, open_channel: 1, channels: [{ id: 1 }],
          presentes: [{ id: 1, nickname: 'QA' }],
        };
        if (m.tipo === 'pedido') valor = JSON.parse(servidor.aoPedir(
          JSON.stringify({ person: '1', channel: 1, admin: true, write: true }),
          JSON.stringify(m.valor),
        ));
        if (m.tipo === 'contribuir') valor = { handle: 1 };
        cliente.aoResponder(JSON.stringify({ n: m.n, ok: true, valor }));
      });
      return true;
    },
  },
});
const escoar = async () => {
  for (let i = 0; i < 30; i++) await new Promise(resolve => setImmediate(resolve));
};
(async () => {
  vm.runInContext(preludio, cliente);
  vm.runInContext(fs.readFileSync(path.join(mod, 'cliente/main.js'), 'utf8'), cliente);
  await escoar();
  assert.ok(medidas.some(m => m.tipo === 'regiao'), 'cliente não terminou a carga inicial');
  cliente.aoResponder(JSON.stringify({ tipo: 'evento', nome: 'acao', acao: 'abrir-aparencia' }));
  await escoar();
  assert.ok(medidas.some(m => m.tipo === 'superficie-montar'), 'abertura não chegou a montar');
  console.log(JSON.stringify({ limiteIndividual: 12288, medidas }, null, 2));
})().catch(erro => { console.error(erro); process.exitCode = 1; });
