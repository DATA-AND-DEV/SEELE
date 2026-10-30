// Máquinas de estado com relógio/RTC simulados. Não prova travessia de NAT;
// rtc-chamada-web e pessoal-codigo-web exercitam os motores RTC reais.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs'), vm = require('node:vm'), path = require('node:path');
const fonte = fs.readFileSync(path.join(__dirname, '../ui/rtc-chamada-web.js'), 'utf8');
const assentar = () => new Promise(setImmediate);
const fingerprint = Array(32).fill(170);
function sdp(geracao, capacidade = true) {
  return 'v=0\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\na=mid:0\r\n'
    + `a=ice-ufrag:${geracao}\r\na=ice-pwd:abcdefghijklmnopqrstuvwx\r\na=fingerprint:sha-256 ${Array(32).fill('AA').join(':')}\r\n`
    + (capacidade ? 'a=x-seele-ice-restart:1\r\n' : '');
}
function candidato(geracao) { return { Candidato: { candidato: `candidate:1 1 UDP 100 192.0.2.1 4000 typ host ufrag ${geracao}`, mid: '0', linha: 0 } }; }
function bancada() {
  let agora = 0, id = 0;
  const tarefas = new Map(), pcs = [];
  const relogio = {
    setTimeout(fn, ms) { const n = ++id; tarefas.set(n, { fn, em: agora + ms }); return n; },
    clearTimeout(n) { tarefas.delete(n); },
    async avancar(ms) {
      const ate = agora + ms;
      for (;;) {
        const proxima = [...tarefas].filter(([, t]) => t.em <= ate).sort((a, b) => a[1].em - b[1].em)[0];
        if (!proxima) break;
        tarefas.delete(proxima[0]); agora = proxima[1].em; proxima[1].fn(); await assentar();
      }
      agora = ate; await assentar();
    },
  };
  class Canal extends EventTarget {
    constructor() { super(); this.readyState = 'connecting'; this.bufferedAmount = 0; this.enviados = []; }
    close() { this.readyState = 'closed'; }
    send(b) { this.enviados.push([...b]); }
  }
  class PC extends EventTarget {
    constructor() {
      super(); this.id = pcs.length; pcs.push(this); this.canais = []; this.ofertas = 0; this.respostas = 0;
      this.signalingState = 'stable'; this.iceGatheringState = 'gathering';
      this.iceConnectionState = 'new'; this.connectionState = 'new'; this.aplicados = [];
    }
    createDataChannel() { const c = new Canal(); this.canais.push(c); return c; }
    async createOffer(opcoes) { this.ofertas++; this.ultimaOferta = opcoes; return { type: 'offer', sdp: sdp(`offer${this.id}x${this.ofertas}`) }; }
    async createAnswer() { this.respostas++; return { type: 'answer', sdp: sdp(`answer${this.id}x${this.respostas}`) }; }
    async setLocalDescription(d) { this.localDescription = d; this.signalingState = d.type === 'offer' ? 'have-local-offer' : 'stable'; }
    async setRemoteDescription(d) { this.remoteDescription = d; this.signalingState = d.type === 'offer' ? 'have-remote-offer' : 'stable'; }
    async addIceCandidate(c) { this.aplicados.push(c); }
    close() { this.connectionState = 'closed'; }
    rede(estado) { this.connectionState = this.iceConnectionState = estado; this.dispatchEvent(new Event('connectionstatechange')); }
    abrir() { this.rede('connected'); for (const c of this.canais) { c.readyState = 'open'; c.dispatchEvent(new Event('open')); } }
    candidato(geracao) {
      const e = new Event('icecandidate'); e.candidate = { candidate: candidato(geracao).Candidato.candidato, sdpMid: '0', sdpMLineIndex: 0, usernameFragment: geracao };
      this.dispatchEvent(e);
    }
  }
  const ctx = vm.createContext({ RTCPeerConnection: PC, ...relogio, Uint8Array, ArrayBuffer, TextEncoder, WritableStream });
  vm.runInContext(fonte + '\nthis.criar = criarEnlaceDaChamadaWeb;', ctx);
  return { criar: ctx.criar, pcs, relogio };
}
async function par() {
  const b = bancada(); let a, z;
  a = b.criar({ papel: 'quem-liga', enviarSinal: s => z.receberSinalAutenticado(s) });
  z = b.criar({ papel: 'quem-recebe', enviarSinal: s => a.receberSinalAutenticado(s) });
  await z.iniciar(); await a.iniciar(); b.pcs.forEach(p => p.abrir()); await Promise.all([a.pronto, z.pronto]);
  return { ...b, a, z };
}
test('STUN incompleto não segura oferta e candidato; prazos são separados e finitos', async () => {
  const b = bancada(), enviados = [];
  const a = b.criar({ papel: 'quem-liga', enviarSinal: async s => enviados.push(s) });
  await a.iniciar(); assert.equal(enviados.length, 1); assert.ok(enviados[0].Oferta);
  b.pcs[0].candidato('offer0x1'); await assentar(); assert.ok(enviados[1].Candidato);
  await b.relogio.avancar(15000); assert.equal(a.estado, 'conectando');
  await a.receberSinalAutenticado({ Resposta: { sdp: sdp('remote1'), impressao: fingerprint } });
  await b.relogio.avancar(29000); assert.notEqual(a.estado, 'encerrada');
  await b.relogio.avancar(1000); assert.equal(a.diagnostico.motivo, 'prazo-ice');
  assert.doesNotMatch(JSON.stringify(a.diagnostico), /192\.0\.2|abcdefghijkl|fingerprint|ufrag/);
});
test('queda transitória preserva chamada; voz não acumula e controle espera recuperação', async () => {
  const { a, z, pcs, relogio } = await par();
  pcs[0].rede('disconnected'); assert.equal(a.estado, 'recuperando');
  assert.equal(a.transporteDaVoz.enviar(new Uint8Array([1])), false);
  let entregue = false; const envio = a.controle.enviar(new Uint8Array([2])).then(() => { entregue = true; });
  await relogio.avancar(4000); assert.equal(entregue, false); assert.equal(pcs[0].ofertas, 1);
  pcs[0].rede('connected'); await envio; assert.equal(a.estado, 'aberta');
  await relogio.avancar(60000); assert.equal(a.estado, 'aberta'); assert.equal(pcs[0].ofertas, 1);
  a.abortar(); z.abortar();
});
test('restart só por quem liga, até duas vezes, preservando DTLS e descartando candidato antigo', async () => {
  const { a, z, pcs, relogio } = await par();
  for (let i = 1; i <= 2; i++) {
    pcs.forEach(p => p.rede('disconnected')); await relogio.avancar(5000);
    assert.equal(pcs[0].ofertas, i + 1); assert.equal(pcs[1].ofertas, 0); assert.equal(pcs[0].ultimaOferta.iceRestart, true);
    const antes = pcs[1].aplicados.length;
    await z.receberSinalAutenticado(candidato('offer0x1')); assert.equal(pcs[1].aplicados.length, antes);
    await z.receberSinalAutenticado(candidato(`offer0x${i + 1}`)); assert.equal(pcs[1].aplicados.length, antes + 1);
    pcs.forEach(p => p.rede('connected')); assert.equal(a.estado, 'aberta'); assert.equal(z.estado, 'aberta');
  }
  pcs.forEach(p => p.rede('disconnected')); await relogio.avancar(45000);
  assert.equal(pcs[0].ofertas, 3); assert.equal(a.estado, 'encerrada'); assert.equal(z.estado, 'encerrada');
});
test('par antigo recebe trickle mas não recebe oferta de restart incompatível', async () => {
  const b = bancada(); const a = b.criar({ papel: 'quem-liga', enviarSinal: async () => {} });
  await a.iniciar(); await a.receberSinalAutenticado({ Resposta: { sdp: sdp('oldpeer', false), impressao: fingerprint } });
  b.pcs[0].abrir(); await a.pronto; b.pcs[0].rede('disconnected');
  await b.relogio.avancar(11000); assert.equal(a.estado, 'recuperando'); assert.equal(b.pcs[0].ofertas, 1);
  await b.relogio.avancar(1000); assert.equal(a.estado, 'encerrada');
});
test('candidatos anteriores ao SDP têm fila limitada e saem na geração correta', async () => {
  const b = bancada(); const z = b.criar({ papel: 'quem-recebe', enviarSinal: async () => {} });
  await z.iniciar(); await z.receberSinalAutenticado(candidato('remote1')); assert.equal(b.pcs[0].aplicados.length, 0);
  await z.receberSinalAutenticado({ Oferta: { sdp: sdp('remote1'), impressao: fingerprint } });
  assert.equal(b.pcs[0].aplicados.length, 1);
  for (let i = 0; i < 64; i++) await z.receberSinalAutenticado(candidato('unknown1'));
  await assert.rejects(z.receberSinalAutenticado(candidato('unknown1')), /demais/);
  assert.equal(z.estado, 'encerrada');
});
test('cancelamento durante sinalização pendente impede callbacks e restart tardios', async () => {
  const b = bancada(); let liberar;
  const a = b.criar({ papel: 'quem-liga', enviarSinal: () => new Promise(ok => { liberar = ok; }) });
  const inicio = a.iniciar(); await assentar(); a.abortar(); await assert.rejects(inicio);
  liberar(); await b.relogio.avancar(90000); assert.equal(a.estado, 'encerrada'); assert.equal(b.pcs[0].ofertas, 1);
});
test('restart não aceita troca de identidade DTLS nem replay de geração antiga', async () => {
  for (const trocarIdentidade of [false, true]) {
    const { a, z } = await par();
    const nova = sdp(trocarIdentidade ? 'newufrag' : 'offer0x1');
    const mensagem = { Oferta: { sdp: trocarIdentidade ? nova.replaceAll('AA', 'BB') : nova, impressao: Array(32).fill(trocarIdentidade ? 187 : 170) } };
    await assert.rejects(z.receberSinalAutenticado(mensagem), /DTLS mudou|reinício inesperado/);
    assert.equal(z.estado, 'encerrada'); a.abortar();
  }
});
