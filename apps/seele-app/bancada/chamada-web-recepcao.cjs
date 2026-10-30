// Node puro: relógio e encontro controlados; WASM de produção e Ed25519 reais.
// Não cobre HTTP Rust, IndexedDB, WebView, ICE nem mídia: essas provas continuam
// nas bancadas pessoal-codigo-web.cjs e chamada-web.cjs com Playwright.
// Uso: SEELE_CHAMADA_WASM=/caminho/seele_web.wasm node este-arquivo.cjs
// Sem a variável, usa o módulo nativo preparado; não compila nem baixa nada.
'use strict';
const assert = require('node:assert/strict');
const { test } = require('node:test');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { webcrypto } = require('node:crypto');
const raiz = path.resolve(__dirname, '../../..');
const arquivoWasm = process.env.SEELE_CHAMADA_WASM || path.join(raiz, 'apps/seele-app/ui/seele_web.wasm');
const modulo = WebAssembly.compile(fs.readFileSync(arquivoWasm));
const fonte = fs.readFileSync(path.join(raiz, 'apps/seele-app/ui/chamada-web.js'), 'utf8');
const agoraReal = Date.now.bind(Date);
const pausaReal = ms => new Promise(ok => setTimeout(ok, ms));
const copiar = valor => JSON.parse(JSON.stringify(valor));
const zeros = n => Array(n).fill(0);
const assinaturaVazia = () => ({ primeira: zeros(32), segunda: zeros(32) });

async function esperar(condicao, descricao) {
  const fim = agoraReal() + 3000;
  while (!condicao()) {
    if (agoraReal() > fim) throw new Error(`Prazo da bancada: ${descricao}`);
    await pausaReal(2);
  }
}
function pendencia() {
  let resolver;
  const promessa = new Promise(ok => { resolver = ok; });
  return { promessa, resolver };
}
function relogio() {
  let agora = 1800000000000, proximo = 0;
  const tarefas = new Map();
  return {
    Date: class extends Date { constructor(...args) { super(...(args.length ? args : [agora])); } static now() { return agora; } },
    now: () => agora,
    setTimeout(fazer, atraso = 0) { const id = ++proximo; tarefas.set(id, { fazer, em: agora + Number(atraso) }); return id; },
    clearTimeout(id) { tarefas.delete(id); },
    // Simula uma aba retomada após o intervalo; timers vencidos executam uma
    // vez, e os novos partem do relógio atual. Evita 43 mil polls para testar 24h.
    async saltar(ms) {
      agora += ms;
      const vencidas = [...tarefas].filter(([, t]) => t.em <= agora).sort((a, b) => a[1].em - b[1].em);
      for (const [id, t] of vencidas) if (tarefas.delete(id)) t.fazer();
      await (this.assentar ? this.assentar() : pausaReal(12));
    },
  };
}
async function criarBancada() {
  const clock = relogio();
  const wasm = new WebAssembly.Instance(await modulo, {}).exports;
  const encoder = new TextEncoder(), decoder = new TextDecoder();
  function operar(intencao) {
    const bytes = encoder.encode(JSON.stringify({ agora: Math.floor(clock.now() / 1000), ...intencao }));
    const p = wasm.alocar(bytes.length);
    try {
      new Uint8Array(wasm.memory.buffer).set(bytes, p); wasm.chamada_operar(p, bytes.length);
      const resposta = JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer, wasm.saida_ptr(), wasm.saida_len()).slice()));
      if (resposta.erro) throw new Error(`WASM real recusou: ${resposta.erro}`);
      return resposta;
    } finally { wasm.liberar(p, bytes.length); }
  }
  async function identidade() {
    const chave = await webcrypto.subtle.generateKey('Ed25519', false, ['sign', 'verify']);
    return { chave, publica: Array.from(new Uint8Array(await webcrypto.subtle.exportKey('raw', chave.publicKey))) };
  }
  const propria = await identidade(), par = await identidade();
  async function assinar(chave, bytes) {
    const assinatura = new Uint8Array(await webcrypto.subtle.sign('Ed25519', chave.privateKey, Uint8Array.from(bytes)));
    return { primeira: Array.from(assinatura.slice(0, 32)), segunda: Array.from(assinatura.slice(32)) };
  }
  async function convite(ident, id = 33) {
    const c = { id: Array(32).fill(id), dono: ident.publica, expira_em: Math.floor(clock.now() / 1000) + 86400, assinatura: assinaturaVazia() };
    c.assinatura = await assinar(ident.chave, operar({ acao: 'convite_mensagem', convite: c }).bytes);
    operar({ acao: 'convite_verificar', convite: c });
    return c;
  }
  const remoto = await convite(par);
  const rede = { epoca: Array(32).fill(7), pedidos: [], gets: 0, registros: [], eventos: [], eventosPorQuemLiga: [], falharBusca: false, recusarEpoca: false, localizar: null, registrar: null, sinal: null, aoBuscar: null, registrosAtivos: new Set() };
  function responder(valor) { return new Response(JSON.stringify(valor), { headers: { 'Content-Type': 'application/json' } }); }
  async function buscar(_url, opcoes) {
    if (opcoes.signal?.aborted) throw new DOMException('Abortada', 'AbortError');
    if (opcoes.method === 'GET') { rede.gets++; return responder({ Desafio: { epoca: rede.epoca, agora: Math.floor(clock.now() / 1000) } }); }
    const p = JSON.parse(opcoes.body);
    // Não se aceita um envelope fake só para exercitar a máquina JS: os bytes
    // canônicos, prazo e assinatura de TODOS os pedidos passam pelo Rust real.
    operar({ acao: 'pedido_verificar', pedido: p });
    rede.pedidos.push(copiar(p));
    if (rede.recusarEpoca || p.epoca.join(',') !== rede.epoca.join(',')) return responder({ Recusa: { motivo: 'Epoca' } });
    if (p.operacao.Registrar) {
      rede.registros.push(copiar(p.operacao.Registrar.convite));
      if (rede.registrar) await rede.registrar.promessa;
      rede.registrosAtivos.add(p.convite.join(','));
      return responder('Ok');
    }
    if (p.operacao === 'Revogar') { rede.registrosAtivos.delete(p.convite.join(',')); return responder('Ok'); }
    if (p.operacao.Localizar) {
      if (rede.localizar) await rede.localizar.promessa;
      return responder({ Localizado: { convite: remoto } });
    }
    if (p.operacao.Sinal && rede.sinal) return responder(await rede.sinal(p));
    if (p.operacao.Buscar) {
      rede.aoBuscar?.(p);
      if (rede.falharBusca === 500) return new Response('Indisponível', { status: 500 });
      if (rede.falharBusca) throw new TypeError('Rede indisponível na bancada');
      const eventos = (p.papel === 'QuemRecebe' ? rede.eventos : rede.eventosPorQuemLiga).splice(0);
      return responder({ Eventos: { eventos, cursor: eventos.at(-1)?.cursor || p.operacao.Buscar.cursor } });
    }
    return responder('Ok');
  }
  let assinaturasEmCurso = 0;
  const cryptoControlada = {
    getRandomValues: bytes => webcrypto.getRandomValues(bytes),
    subtle: {
      sign: (...args) => {
        assinaturasEmCurso++;
        return webcrypto.subtle.sign(...args).finally(() => { assinaturasEmCurso--; });
      },
    },
  };
  // Espera o pool real de cripto esvaziar, sem assumir um prazo de assinatura
  // da máquina de CI. setImmediate deixa as continuações das Promises rodarem.
  clock.assentar = async () => {
    const fim = agoraReal() + 3000;
    let ociosas = 0;
    while (ociosas < 2) {
      if (agoraReal() > fim) throw Error('Assinaturas não concluíram na bancada');
      await new Promise(ok => setImmediate(ok));
      ociosas = assinaturasEmCurso ? 0 : ociosas + 1;
    }
  };
  const estados = [], recepcoes = [], opcoesDosEnlaces = [];
  let enlaces = 0;
  const contexto = vm.createContext({ crypto: cryptoControlada, TextEncoder, TextDecoder, AbortController, DOMException,
    Date: clock.Date, setTimeout: clock.setTimeout, clearTimeout: clock.clearTimeout,
    location: { origin: 'https://seele.test', href: 'https://seele.test/' }, URL,
    criarEnlaceDaChamadaWeb: () => { throw Error('adaptador não injetado'); } });
  vm.runInContext(fonte + '\nglobalThis.criar = criarServicoDeChamadasWeb;', contexto);
  const servico = await contexto.criar({ wasm, buscar, identidade: propria,
    conferirPar: async (codigo, publica) => {
      assert.equal(operar({ acao: 'codigo_da_chave', chave: publica }).codigo, codigo);
    },
    criarEnlace: opcoes => {
      opcoesDosEnlaces.push(opcoes); enlaces++;
      return { controle: { ouvir() {}, enviar() {} }, iniciar: async () => {}, pronto: Promise.resolve(), receberSinalAutenticado: async () => {}, abortar() {} };
    },
    aoEstado: e => estados.push(copiar(e)), aoRecepcao: e => recepcoes.push(copiar(e)),
  });
  async function registrar() {
    await servico.receberPorCodigo({ apelido: 'Pessoa da bancada' });
    await esperar(() => rede.pedidos.some(p => p.operacao.Buscar), 'primeiro polling');
    await clock.assentar();
  }
  async function chamadaRecebida() {
    const c = rede.registros.at(-1);
    const p = { papel: 'QuemLiga', epoca: rede.epoca, nonce: Array(16).fill(8), convite: c.id, chamada: Array(32).fill(9),
      remetente: par.publica, destinatario: propria.publica, geracao: 1, sequencia: 1,
      valido_ate: Math.floor(clock.now() / 1000) + 60, operacao: { Chamar: { convite: c, apelido: 'Contato' } }, assinatura: assinaturaVazia() };
    p.assinatura = await assinar(par.chave, operar({ acao: 'pedido_mensagem', pedido: p }).bytes);
    operar({ acao: 'pedido_verificar', pedido: p });
    rede.eventos.push({ pedido: p, cursor: 1 });
    await clock.saltar(2000);
    await esperar(() => servico.ler().estado === 'tocando', 'chamada recebida assinada');
  }
  async function chamadaFeita() {
    await servico.chamarCodigo({ codigo: operar({ acao: 'codigo_da_chave', chave: par.publica }).codigo, apelido: 'Pessoa' });
    await clock.assentar();
    const enviada = rede.pedidos.findLast(p => p.operacao.Chamar);
    const p = { papel: 'QuemRecebe', epoca: rede.epoca, nonce: Array(16).fill(12), convite: enviada.convite,
      chamada: enviada.chamada, remetente: par.publica, destinatario: propria.publica, geracao: enviada.geracao,
      sequencia: 1, valido_ate: Math.floor(clock.now() / 1000) + 60, operacao: 'Atender', assinatura: assinaturaVazia() };
    p.assinatura = await assinar(par.chave, operar({ acao: 'pedido_mensagem', pedido: p }).bytes);
    rede.eventosPorQuemLiga.push({ pedido: p, cursor: 1 });
    await clock.saltar(2000);
    await esperar(() => servico.ler().estado === 'dentro', 'chamada feita admitida por resposta assinada');
  }
  async function esgotarRetry() {
    // Mantém a falha nos três backoffs limitados. O tempo real só permite que
    // Ed25519 termine; a passagem de segundos/minutos é inteiramente fake.
    for (const intervalo of [2100, 5100, 15100, 16000]) await clock.saltar(intervalo);
  }
  return { clock, rede, servico, estados, recepcoes, registrar, chamadaRecebida, chamadaFeita, esgotarRetry, opcoesDosEnlaces,
    remoto, codigoPar: operar({ acao: 'codigo_da_chave', chave: par.publica }).codigo,
    enlaces: () => enlaces, fechar: () => servico.destruir() };
}

test('Localizar atrasado não apaga uma chamada recebida que tomou a tela', async () => {
  const b = await criarBancada();
  try {
    await b.registrar(); b.rede.localizar = pendencia();
    const busca = b.servico.chamarCodigo({ codigo: b.codigoPar, apelido: 'Pessoa' }).then(() => null, erro => erro);
    await esperar(() => b.rede.pedidos.some(p => p.operacao.Localizar), 'Localizar em voo');
    await b.chamadaRecebida(); b.rede.localizar.resolver(); await busca;
    assert.equal(b.servico.ler().estado, 'tocando', 'a conclusão da busca não governa a chamada recebida');
    assert.equal(await b.servico.atender(), true);
  } finally { b.fechar(); }
});

test('recepção por código renova antes de 24h sem mudar a identidade', async () => {
  const b = await criarBancada();
  try {
    await b.registrar(); const original = b.rede.registros.at(-1), codigo = b.servico.codigo;
    await b.clock.saltar(86400000 - 30000);
    await esperar(() => b.rede.registros.length >= 2, 'renovação antes da expiração');
    const nova = b.rede.registros.at(-1);
    assert.ok(b.clock.now() < original.expira_em * 1000, 'a renovação ocorre antes do prazo antigo');
    assert.ok(nova.expira_em > original.expira_em);
    assert.notDeepEqual(nova.id, original.id);
    assert.deepEqual(nova.dono, original.dono); assert.equal(b.servico.codigo, codigo);
    assert.equal(b.recepcoes.at(-1).estado, 'registrada');
  } finally { b.fechar(); }
});

test('falha de rede ociosa anuncia indisponibilidade e permite retry explícito', async () => {
  const b = await criarBancada();
  try {
    await b.registrar();
    b.rede.falharBusca = true; await b.clock.saltar(2000);
    await esperar(() => b.recepcoes.some(r => r.estado === 'indisponivel'), 'aviso de recepção indisponível');
    await b.esgotarRetry(); const depoisDasFalhas = b.rede.pedidos.length;
    await b.clock.saltar(60000);
    assert.equal(b.rede.pedidos.length, depoisDasFalhas, 'o backoff esgotado não vira repetição sem limite');
    const antes = b.rede.registros.length, codigo = b.servico.codigo, enlaces = b.enlaces();
    b.rede.falharBusca = false; await b.servico.receberPorCodigo({ apelido: 'Pessoa' });
    await esperar(() => b.rede.registros.length > antes, 'Registrar no retry explícito');
    await esperar(() => b.recepcoes.at(-1)?.estado === 'registrada', 'recepção recuperada');
    assert.equal(b.servico.codigo, codigo); assert.equal(b.enlaces(), enlaces, 'retry de recepção não reconecta mídia');
    const ultimoRegistro = b.rede.pedidos.findLastIndex(p => p.operacao.Registrar);
    await esperar(() => b.rede.pedidos.slice(ultimoRegistro + 1).some(p => p.operacao.Buscar && p.operacao.Buscar.cursor === 0), 'cursor reiniciado depois de Registrar');
  } finally { b.fechar(); }
});

for (const lado of ['quem liga', 'quem recebe']) {
  async function admitir(b) {
    if (lado === 'quem liga') await b.chamadaFeita();
    else {
      await b.registrar(); await b.chamadaRecebida(); await b.servico.atender();
      await esperar(() => b.servico.ler().estado === 'dentro', 'transporte recebido admitido');
    }
  }
  test(`sinalização de ${lado} retoma após esgotar backoff sem recriar transporte`, async () => {
    const b = await criarBancada();
    try {
      await admitir(b); const enlaces = b.enlaces();
      b.rede.falharBusca = 500; await b.clock.saltar(2000); await b.esgotarRetry();
      const contar = () => b.rede.pedidos.filter(p => p.operacao.Buscar).length;
      const antes = contar(); await b.clock.saltar(15000);
      assert.ok(contar() > antes, 'chamada admitida continua tentando com teto de 15 segundos');
      b.rede.falharBusca = false; const falhas = contar(); await b.clock.saltar(15000);
      assert.ok(contar() > falhas, 'a volta da rede alcança novo polling');
      if (lado === 'quem recebe') assert.equal(b.recepcoes.at(-1).estado, 'registrada');
      assert.equal(b.servico.ler().estado, 'dentro'); assert.equal(b.enlaces(), enlaces);
      const sucesso = contar(); await b.clock.saltar(2000);
      assert.ok(contar() > sucesso, 'polling volta ao intervalo normal depois do sucesso');
    } finally { b.fechar(); }
  });
  test(`encerrar cancela continuação do backoff de ${lado}`, async () => {
    const b = await criarBancada();
    try {
      await admitir(b); b.rede.falharBusca = 500; await b.clock.saltar(2000); await b.esgotarRetry();
      b.servico.encerrar(); await b.clock.assentar();
      const contar = () => b.rede.pedidos.filter(p => p.operacao.Buscar || p.operacao.Registrar).length;
      const antes = contar(); await b.clock.saltar(60000);
      assert.equal(contar(), antes, 'encerramento não deixa retry de sinalização admitida rodando');
    } finally { b.fechar(); }
  });
}

function candidatoDaBancada() {
  return { Candidato: { candidato: 'candidate:1 1 UDP 100 192.0.2.1 4000 typ host ufrag testgen', mid: '0', linha: 0 } };
}
function pedidosDeSinal(b) { return b.rede.pedidos.filter(p => p.operacao.Sinal); }

test('Cheio usa novo nonce e deixa polling drenar durante espera fora da fila', async () => {
  const b = await criarBancada();
  try {
    await b.chamadaFeita(); let cheia = true, recusas = 0;
    b.rede.sinal = () => { if (cheia) { recusas++; return { Recusa: { motivo: 'Cheio' } }; } return 'Ok'; };
    b.rede.aoBuscar = () => { cheia = false; };
    const envio = b.opcoesDosEnlaces.at(-1).enviarSinal(candidatoDaBancada());
    await b.clock.saltar(499); assert.equal(pedidosDeSinal(b).length, 0, 'candidato respeita pacing inicial de 500ms');
    await b.clock.saltar(1); assert.equal(pedidosDeSinal(b).length, 1);
    await b.clock.saltar(1000); assert.equal(pedidosDeSinal(b).length, 2);
    await b.clock.saltar(500); assert.equal(cheia, false, 'Buscar executa antes do retry pendente terminar');
    await b.clock.saltar(500); await envio;
    const sinais = pedidosDeSinal(b); assert.equal(recusas, 2); assert.equal(sinais.length, 3);
    assert.equal(new Set(sinais.map(p => p.nonce.join(','))).size, 3, 'recusas explícitas exigem nonces novos');
    assert.equal(new Set(sinais.map(p => JSON.stringify(p.assinatura))).size, 3);
    assert.ok(sinais.every(p => JSON.stringify(p.operacao) === JSON.stringify(sinais[0].operacao)));
    const primeiro = b.rede.pedidos.indexOf(sinais[0]), ultimo = b.rede.pedidos.indexOf(sinais.at(-1));
    assert.ok(b.rede.pedidos.slice(primeiro + 1, ultimo).some(p => p.operacao.Buscar));
  } finally { b.fechar(); }
});

test('Cheio persistente tem somente uma tentativa inicial e cinco retries', async () => {
  const b = await criarBancada();
  try {
    await b.chamadaFeita(); b.rede.sinal = () => ({ Recusa: { motivo: 'Cheio' } });
    const resultado = b.opcoesDosEnlaces.at(-1).enviarSinal(candidatoDaBancada()).then(() => null, erro => erro);
    await b.clock.saltar(500);
    for (let i = 0; i < 5; i++) await b.clock.saltar(1000);
    assert.equal((await resultado)?.motivo, 'Cheio');
    const sinais = pedidosDeSinal(b); assert.equal(sinais.length, 6);
    assert.equal(new Set(sinais.map(p => p.nonce.join(','))).size, 6);
    await b.clock.saltar(60000); assert.equal(pedidosDeSinal(b).length, 6, 'erro final não deixa retry tardio');
  } finally { b.fechar(); }
});

for (const fase of ['pacing', 'retry de Cheio']) test(`cancelamento durante ${fase} impede sinal tardio`, async () => {
  const b = await criarBancada();
  try {
    await b.chamadaFeita(); b.rede.sinal = () => ({ Recusa: { motivo: 'Cheio' } });
    const resultado = b.opcoesDosEnlaces.at(-1).enviarSinal(candidatoDaBancada()).then(() => null, erro => erro);
    if (fase === 'retry de Cheio') await b.clock.saltar(500);
    const enviados = pedidosDeSinal(b).length; assert.equal(enviados, fase === 'pacing' ? 0 : 1);
    b.servico.encerrar(); assert.equal((await resultado)?.name, 'AbortError');
    await b.clock.saltar(60000); assert.equal(pedidosDeSinal(b).length, enviados);
  } finally { b.fechar(); }
});

test('restart da época refaz uma vez com novo nonce e assinatura válidos', async () => {
  const b = await criarBancada();
  try {
    await b.registrar(); const antes = b.rede.pedidos.length, gets = b.rede.gets;
    b.rede.epoca = Array(32).fill(11); await b.clock.saltar(2000);
    await esperar(() => b.rede.pedidos.slice(antes).some(p => p.epoca[0] === 11), 'pedido refeito com época nova');
    const pedidos = b.rede.pedidos.slice(antes), antigo = pedidos.find(p => p.epoca[0] === 7), novo = pedidos.find(p => p.epoca[0] === 11);
    assert.ok(antigo && novo); assert.equal(b.rede.gets, gets + 1);
    assert.notDeepEqual(novo.nonce, antigo.nonce); assert.notDeepEqual(novo.assinatura, antigo.assinatura);
    assert.equal(JSON.stringify(novo.operacao), JSON.stringify(antigo.operacao));
  } finally { b.fechar(); }
  const repetida = await criarBancada();
  try {
    repetida.rede.recusarEpoca = true;
    await assert.rejects(repetida.servico.receberPorCodigo({ apelido: 'Pessoa' }));
    assert.equal(repetida.rede.gets, 2, 'uma descoberta inicial e só uma redescoberta');
    assert.equal(repetida.rede.pedidos.length, 2, 'uma tentativa e só um retry de época');
  } finally { repetida.fechar(); }
});


test('desligar durante Registrar invalida a operação e revoga a conclusão tardia', async () => {
  const b = await criarBancada();
  try {
    b.rede.registrar = pendencia();
    const registro = b.servico.receberPorCodigo({ apelido: 'Pessoa' }).then(() => null, erro => erro);
    await esperar(() => b.rede.registros.length === 1, 'Registrar pendente');
    const desligar = b.servico.revogarConvite();
    assert.equal(b.recepcoes.at(-1).estado, 'desligada');
    const antes = b.estados.length;
    b.rede.registrar.resolver();
    const erro = await registro; await desligar;
    assert.equal(erro?.name, 'AbortError', 'uma criação cancelada não retorna um convite utilizável');
    assert.equal(b.rede.registrosAtivos.size, 0, 'Registrar tardio foi revogado no encontro');
    assert.equal(b.rede.pedidos.filter(p => p.operacao === 'Revogar').length, 1);
    assert.equal(b.recepcoes.at(-1).estado, 'desligada');
    assert.ok(!b.estados.slice(antes).some(e => e.estado === 'recebendo'), 'a conclusão tardia não reabre a recepção');
    assert.ok(!b.rede.pedidos.some(p => p.operacao.Buscar), 'a caixa cancelada não inicia polling');
    const pedidos = b.rede.pedidos.length;
    await b.clock.saltar(86400000);
    assert.equal(b.rede.pedidos.length, pedidos, 'a caixa cancelada não renova nem faz retry');
  } finally { b.fechar(); }
});


test('OFF e ON durante Registrar esperam a revogação e criam uma recepção nova', async () => {
  const b = await criarBancada();
  try {
    b.rede.registrar = pendencia();
    const antiga = b.servico.receberPorCodigo({ apelido: 'Pessoa' }).then(() => null, erro => erro);
    await esperar(() => b.rede.registros.length === 1, 'Registrar antigo em voo');
    const desligar = b.servico.revogarConvite();
    const nova = b.servico.receberPorCodigo({ apelido: 'Pessoa' });
    b.rede.registrar.resolver();
    await desligar; assert.equal((await antiga)?.name, 'AbortError');
    await nova;
    await esperar(() => b.rede.registros.length === 2, 'novo Registrar depois de ON');
    assert.notDeepEqual(b.rede.registros[0].id, b.rede.registros[1].id);
    assert.equal(b.rede.registrosAtivos.size, 1);
    assert.ok(b.rede.registrosAtivos.has(b.rede.registros[1].id.join(',')));
    assert.equal(b.recepcoes.at(-1).estado, 'registrada');
    await b.servico.chamarCodigo({ codigo: b.codigoPar, apelido: 'Pessoa' });
    await b.clock.assentar();
    assert.equal(b.servico.ler().estado, 'chamando', 'a revogação antiga não encerra a chamada nova');
  } finally { b.fechar(); }
});
