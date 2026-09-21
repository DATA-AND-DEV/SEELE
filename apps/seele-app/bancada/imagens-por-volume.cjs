// Executa a ponte real: capacidade, identidade, confirmação e saída de sessão.
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const assert = require('node:assert/strict');
const base = fs.readFileSync(path.join(__dirname, '../ui/base.js'), 'utf8');
const ponte = base.slice(base.indexOf('const CAPACIDADES_POR_API'), base.indexOf('const modsCarregados ='));
const rust = fs.readFileSync(path.join(__dirname, '../src/executor.rs'), 'utf8');
const preludio = rust.split('const PRELUDIO: &str = r#"')[1].split('"#;')[0];
(async () => {
  const chamadas = [], respostas = [];
  let concluir, ativa = true;
  const instancia = { admite: () => ativa, executor: { entregar: r => respostas.push(r) } };
  const ctx = vm.createContext({
    modsCarregados: new Map([['seele/perfis', instancia]]), geracaoDaSessao: 8,
    registrarNoAnfitriao() {},
    invoke: (cmd, args) => { chamadas.push({cmd, args}); return new Promise(resolve => { concluir = resolve; }); },
  });
  vm.runInContext(ponte, ctx);
  for (const api of [3, 4]) {
    await ctx.atenderOMod({id:'seele/perfis',api}, instancia, {n:api,tipo:'enviar-imagem',arquivo:1,token:'t'});
    assert.equal(respostas.at(-1).ok, false);
  }
  assert.equal(chamadas.length, 0);
  const envio = ctx.atenderOMod({id:'seele/perfis',api:5}, instancia,
    {n:5,tipo:'enviar-imagem',arquivo:11,token:'t',id:'outro/mod',geracao:999});
  assert.equal(chamadas[0].cmd, 'enviar_imagem_mod');
  assert.equal(chamadas[0].args.id, 'seele/perfis');
  assert.equal(chamadas[0].args.geracao, 8);
  assert.equal(respostas.length, 2, 'confirmação anterior à gravação');
  concluir(); await envio;
  assert.equal(respostas.at(-1).ok, true);
  const tardio = ctx.atenderOMod({id:'seele/perfis',api:5}, instancia, {n:6,tipo:'enviar-imagem',arquivo:12,token:'t2'});
  ativa = false; concluir(); await tardio;
  assert.equal(respostas.at(-1).erro, 'sessao-encerrada');

  for (const volume of [false, true]) {
    const mensagens = [];
    const sandbox = vm.createContext({
      __seeleCapacidades: ['regiao', 'arquivo', ...(volume ? ['volume'] : [])],
      seele: { postar: texto => { mensagens.push(JSON.parse(texto)); return true; } },
    });
    vm.runInContext(preludio, sandbox);
    assert.equal(typeof sandbox.SeeleUI.enviar, volume ? 'function' : 'undefined');
    if (volume) {
      const promessa = sandbox.SeeleUI.enviar(11, 'token');
      assert.equal(mensagens[0].tipo, 'enviar-imagem');
      sandbox.aoResponder(JSON.stringify({tipo:'resposta',n:mensagens[0].n,ok:true,valor:null}));
      await promessa;
    }
  }
  console.log('volume: SDK e ponte reais conferem capacidade, dono, gravação e geração.');
})().catch(erro => { console.error(erro); process.exitCode = 1; });
