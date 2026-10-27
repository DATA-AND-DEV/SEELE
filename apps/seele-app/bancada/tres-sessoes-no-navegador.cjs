// Três sessões web ao mesmo tempo (E3), num navegador real, contra três
// servidores de verdade (`crates/seele-server/examples/servidor_da_bancada.rs`).
//
//   node apps/seele-app/bancada/tres-sessoes-no-navegador.cjs <seele_web.wasm> <servidor_da_bancada> <chromium|webkit> <playwright> [--json saída]
//
// A página carrega o `rtc-web.js` e o `sessao-web.js` de produto, e o registro
// de sessões (`criarRegistroDeSessoesWeb`) é o de produto. A voz da página é
// trocada por uma voz de mentira que anota quem tem o áudio, mas os datagramas
// saem pelo transporte de verdade de cada sessão, e o portão é o de produto.
//
// O que se prova:
// 1. **três sessões conectadas**, uma em cada servidor, com o foco na primeira;
// 2. **trocar a vista sem desconectar**: as três continuam `online`;
// 3. **o foco de voz único**: com os três transportes tentando mandar voz a
//    cada 20 ms durante as trocas, em nenhuma volta dois saem, e cada servidor
//    só recebe a voz enquanto a sessão dele tem o foco. O áudio nunca tem dois
//    donos;
// 4. **o evento tardio é da sessão dele**: uma mensagem mandada em A depois de
//    a vista ir para B chega carimbada com A;
// 5. **a promessa depois da troca**: o comando pedido em A antes da troca
//    resolve pelo handle de A;
// 6. **a volta em segundo plano**: o transporte de C cai com a vista em A; C
//    volta sozinho, e A e B não ouvem evento de C;
// 7. **o anexo de A durante a troca para B**: ele sobe pelo handle de A, chega
//    à conversa de A e não à de B;
// 8. **o clique durante a troca**: um `focarVoz` pedido no meio de outro espera
//    a vez, e o resultado é o do último pedido.

"use strict";

const fs = require("fs");
const http = require("http");
const path = require("path");
const readline = require("readline");
const { spawn } = require("child_process");

const argumentos = process.argv.slice(2);
const opcao = (nome) => (argumentos.includes(nome) ? argumentos[argumentos.indexOf(nome) + 1] : null);
const [wasm, servidorDaBancada, navegador = "chromium", playwright] = argumentos.filter(
  (a, i) => !a.startsWith("--") && !(i > 0 && argumentos[i - 1] === "--json"),
);
const saidaJson = opcao("--json");
if (!wasm || !servidorDaBancada || !playwright) {
  console.error("uso: node tres-sessoes-no-navegador.cjs <seele_web.wasm> <servidor_da_bancada> <chromium|webkit> <playwright> [--json saída]");
  process.exit(2);
}
const UI = path.join(__dirname, "..", "ui");

function subirUmServidor() {
  const processo = spawn(servidorDaBancada, [], { stdio: ["pipe", "pipe", "inherit"] });
  return new Promise((ok, falha) => {
    readline.createInterface({ input: processo.stdout }).once("line", (linha) => ok({ processo, ...JSON.parse(linha) }));
    processo.once("exit", (codigo) => falha(new Error(`o servidor da bancada saiu com ${codigo}`)));
  });
}

function servirAPagina() {
  const servidor = http.createServer((pedido, resposta) => {
    const arquivos = {
      "/rtc-web.js": [path.join(UI, "rtc-web.js"), "text/javascript"],
      "/sessao-web.js": [path.join(UI, "sessao-web.js"), "text/javascript"],
      "/seele_web.wasm": [wasm, "application/wasm"],
    };
    const achado = arquivos[pedido.url];
    if (achado) {
      resposta.writeHead(200, { "Content-Type": achado[1] });
      resposta.end(fs.readFileSync(achado[0]));
      return;
    }
    resposta.writeHead(200, { "Content-Type": "text/html" });
    resposta.end('<!doctype html><meta charset="utf-8"><script src="/rtc-web.js"></script><script src="/sessao-web.js"></script>');
  });
  return new Promise((ok) => servidor.listen(0, "127.0.0.1", () => ok(servidor)));
}

/** Na página: as oito provas. */
async function provarNaPagina({ links }) {
  const R = { casos: {} };
  const espera = (ms) => new Promise((r) => setTimeout(r, ms));
  const bytes = await (await fetch("/seele_web.wasm")).arrayBuffer();
  const instanciar = async () => (await WebAssembly.instantiate(bytes, {})).instance.exports;
  const leitor = await instanciar();
  const ler = (texto) => {
    const b = new TextEncoder().encode(texto);
    const p = leitor.alocar(b.length);
    new Uint8Array(leitor.memory.buffer).set(b, p);
    try {
      leitor.analisar_link(p, b.length);
    } finally {
      leitor.liberar(p, b.length);
    }
    return JSON.parse(new TextDecoder().decode(new Uint8Array(leitor.memory.buffer, leitor.saida_ptr(), leitor.saida_len()).slice()));
  };

  // A voz de mentira: quem tem o áudio, e se alguma vez foram dois.
  const donos = new Set();
  let doisDonos = 0;
  const voz = {
    async soltar(id) {
      await espera(30);
      donos.delete(id);
    },
    async tomar(id) {
      if (donos.size > 0) doisDonos += 1;
      donos.add(id);
    },
  };
  const eventos = [];
  const registro = criarRegistroDeSessoesWeb({ instanciar, voz });

  // ------------------------------------------------------- 1. três entradas
  const ids = [];
  for (const texto of links) {
    const { link } = ler(texto);
    const chave = await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]);
    const alvo = { endereco: link.alvo, alternativos: link.alternativos ?? [], impressao: link.impressao, bilhete: link.bilhete ?? null, chave };
    const { id } = await registro.entrar({
      alvo,
      apelido: `ana${ids.length + 1}`,
      convite: link.convite ?? null,
      emitir: (carimbado) => eventos.push(carimbado),
    });
    ids.push(id);
  }
  const [A, B, C] = ids;
  R.casos.entradas = {
    ids,
    sessoes: registro.sessoes().map((s) => ({ id: s.id, enlace: s.enlace, a_frente: s.a_frente, foco: s.foco_de_voz })),
    foco: registro.foco(),
  };

  // ------------------------------------------- 3. a voz durante as trocas
  const voltas = [];
  let mandando = true;
  const quadro = new Uint8Array(80).fill(1);
  const relogioDaVoz = (async () => {
    while (mandando) {
      const saiu = ids.filter((id) => registro.sessao(id)?.transporteDaVoz.enviar(quadro));
      voltas.push({ foco: registro.foco(), saiu, donos: donos.size });
      await espera(20);
    }
  })();
  await espera(300);
  // 8. dois cliques seguidos: B e depois C; vale o último.
  const primeiro = registro.focarVoz(B);
  const segundo = registro.focarVoz(C);
  const [r1, r2] = await Promise.all([primeiro, segundo]);
  await espera(300);
  await registro.focarVoz(A);
  await espera(300);
  mandando = false;
  await relogioDaVoz;
  const contas = Object.fromEntries(ids.map((id) => [id, { ...registro.sessao(id).contas }]));
  R.casos.voz = {
    voltas: voltas.length,
    voltasComDoisSaindo: voltas.filter((v) => v.saiu.length > 1).length,
    voltasComSaidaForaDoFoco: voltas.filter((v) => v.saiu.some((id) => id !== v.foco)).length,
    doisDonos,
    cliques: [r1, r2],
    focoNoFim: registro.foco(),
    contas,
  };

  // ------------------------------------------ 2. trocar a vista sem cair
  registro.ver(A);
  registro.ver(B);
  await espera(200);
  R.casos.vista = registro.sessoes().map((s) => ({ id: s.id, enlace: s.enlace, a_frente: s.a_frente }));

  // --------------------- 4 e 5. o evento tardio e a promessa depois da troca
  const sessaoA = registro.sessao(A);
  const canal = sessaoA.retrato()?.sala?.channels?.[0]?.id ?? 0;
  sessaoA.atender("open_channel", { channel: canal });
  registro.ver(B);
  const antes = eventos.length;
  const resposta = sessaoA.atender("send_message", { channel: canal, body: "de A, depois da troca" });
  await espera(800);
  const depois = eventos.slice(antes).filter((e) => e.evento === "MessagesChanged");
  R.casos.eventoTardio = {
    resposta: resposta?.resposta ?? null,
    mensagensCarimbadas: depois.map((e) => e.sessao),
    soDeA: depois.length > 0 && depois.every((e) => e.sessao === A),
  };

  // --------------------------------------- 6. a volta em segundo plano
  registro.ver(A);
  const marca = eventos.length;
  registro.sessao(C).simularAQueda();
  const ate = performance.now() + 15_000;
  while (performance.now() < ate && (registro.sessao(C).voltando || registro.sessao(C).contas.voltas === 0)) await espera(100);
  const vistos = eventos.slice(marca);
  R.casos.voltaAoFundo = {
    voltou: registro.sessao(C).contas.voltas >= 1 && !registro.sessao(C).voltando,
    eventosDeOutros: vistos.filter((e) => e.sessao !== C && e.evento && e.evento.Ended).length,
    sessoes: registro.sessoes().map((s) => ({ id: s.id, enlace: s.enlace })),
  };

  // ------------------------------- 7. o anexo de A durante a troca para B
  {
    const conteudo = new Uint8Array(512 * 1024).fill(7);
    const hash = Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", conteudo)), (b) => b.toString(16).padStart(2, "0")).join("");
    const cabecalho = sessaoA.atender("cabecalho_do_anexo", { channel: canal, body: "anexo de A", nome: "a.bin", tipo: "", tamanho: conteudo.length, hash });
    const feito = cabecalho.resposta ?? {};
    const escritor = (await sessaoA.abrirUmFluxo()).getWriter();
    await escritor.write(new Uint8Array(feito.cabecalho ?? []));
    for (let i = 0; i < conteudo.length; i += 64 * 1024) {
      if (i === 128 * 1024) registro.ver(B);
      await escritor.write(conteudo.subarray(i, i + 64 * 1024));
      await espera(20);
    }
    await escritor.close();
    await espera(1500);
    const deA = sessaoA.atender("messages", {})?.resposta ?? [];
    const deB = registro.sessao(B).atender("messages", {})?.resposta ?? [];
    const temAnexo = (lista) => lista.some((m) => JSON.stringify(m).includes("a.bin"));
    R.casos.anexoNaTroca = { recusa: feito.recusa ?? null, emA: temAnexo(deA), emB: temAnexo(deB) };
  }

  for (const id of ids) await registro.encerrar(id);
  return R;
}

(async () => {
  const servidores = [];
  for (let i = 0; i < 3; i += 1) servidores.push(await subirUmServidor());
  const pagina = await servirAPagina();
  const { chromium, webkit } = require(playwright);
  const browser = await (navegador === "webkit" ? webkit : chromium).launch();
  let R;
  try {
    const aba = await browser.newPage();
    aba.on("console", (m) => {
      if (m.type() === "error") console.error(`[página] ${m.text()}`);
    });
    await aba.goto(`http://127.0.0.1:${pagina.address().port}/`);
    R = await aba.evaluate(provarNaPagina, { links: servidores.map((s) => s.link) });
    R.navegador = `${navegador} ${browser.version()}`;
  } finally {
    await browser.close();
    pagina.close();
    for (const s of servidores) s.processo.stdin.end();
  }
  const c = R.casos;
  R.veredito = {
    tres: c.entradas.sessoes.length === 3 && c.entradas.sessoes.every((s) => s.enlace === "online") && c.entradas.foco === c.entradas.ids[0],
    vista: c.vista.every((s) => s.enlace === "online"),
    vozUnica: c.voz.voltasComDoisSaindo === 0 && c.voz.voltasComSaidaForaDoFoco === 0 && c.voz.doisDonos === 0,
    cliques: c.voz.cliques[1]?.agora === c.entradas.ids[2] && c.voz.focoNoFim === c.entradas.ids[0],
    eventoTardio: c.eventoTardio.soDeA,
    voltaAoFundo: c.voltaAoFundo.voltou && c.voltaAoFundo.eventosDeOutros === 0,
    anexoNaTroca: c.anexoNaTroca.emA && !c.anexoNaTroca.emB,
  };
  const texto = JSON.stringify(R, null, 2);
  console.log(texto);
  if (saidaJson) fs.writeFileSync(saidaJson, texto + "\n");
  process.exit(Object.values(R.veredito).every(Boolean) ? 0 : 1);
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
