// **Os cargos pela página, contra servidores de verdade** (E6 da 0.17).
//
//   node apps/seele-app/bancada/cargos-no-navegador.cjs <seele_web.wasm> <servidor_da_bancada> <chromium|webkit> <playwright> [--json saída] [--sem-cargos-no-wasm]
//
// Os nove comandos de cargos (`pedir_cargos`, `explicar_permissoes`,
// `prever_cargo` e os seis verbos) saem da sessão web de produto
// (`sessao-web.js`, `criarRegistroDeSessoesWeb`) para o `.wasm` de protocolo
// DAQUELA sessão (`seele_web::comandos`), que lê o `Room` dela e escreve no fio.
// Do outro lado há dois `servidor_da_bancada` reais, com o backend de cargos
// do F: S, com a dona declarada por `--proprietario`, e T, sem dono.
//
// A identidade de cada pessoa é fixada pela bancada (`identidade-da-bancada.cjs`):
// a impressão da dona sai da página ANTES de S subir, e é ela que o
// `--proprietario` declara. O produto não muda.
//
// O que se prova:
// 1. **a dona é a dona**: a impressão da página é a que S declarou, e o
//    `pedir_cargos` da sessão em S diz `proprietario: true`; em T, não;
// 2. **o verbo vai ao servidor e volta pelo quadro**: `criar_cargo` em S cria
//    o cargo, a revisão sobe, e o quadro de T não muda;
// 3. **a prévia é a do servidor**: `prever_cargo` diz quem ganha o quê antes,
//    e com a revisão velha dá a `RevisaoVelha` que o servidor daria;
// 4. **atribuir chega à outra pessoa**: a Bia (outra página, outra chave) passa
//    a ter as permissões do cargo, vistas pela sessão DELA;
// 5. **a recusa é do servidor**: a Bia sem GERIR CARGOS pede `criar_cargo`, e
//    a recusa nomeada chega à sessão dela (`ultima_recusa`);
// 6. **por sessão**: a explicação da dona em S e em T são as de cada quadro;
// 7. **o aviso da mudança**: a sessão em S avisa `CargosChanged { marca }` (o
//    `Event::CargosChanged` do desktop) depois do `criar_cargo`, e a marca é o
//    `cargos_revision` do retrato DELA; a sessão em T não ouve nada disso.
//
// `--sem-cargos-no-wasm` serve à página um `.wasm` cujos nomes de cargo não
// existem (a reversão: os comandos voltam `null`, isto é, «comando
// desconhecido»), e exige que as provas reprovem.

"use strict";

const fs = require("fs");
const http = require("http");
const path = require("path");
const readline = require("readline");
const { spawn } = require("child_process");
const { fixarAIdentidade, impressaoDaPagina } = require("./identidade-da-bancada.cjs");

const argumentos = process.argv.slice(2);
const opcao = (nome) => (argumentos.includes(nome) ? argumentos[argumentos.indexOf(nome) + 1] : null);
const [wasm, servidorDaBancada, navegador = "chromium", playwright] = argumentos.filter(
  (a, i) => !a.startsWith("--") && !(i > 0 && argumentos[i - 1] === "--json"),
);
const saidaJson = opcao("--json");
const REVERTER = argumentos.includes("--sem-cargos-no-wasm");
if (!wasm || !servidorDaBancada || !playwright) {
  console.error("uso: node cargos-no-navegador.cjs <seele_web.wasm> <servidor_da_bancada> <chromium|webkit> <playwright> [--json saída] [--sem-cargos-no-wasm]");
  process.exit(2);
}
const UI = path.join(__dirname, "..", "ui");

function subirUmServidor(proprietario) {
  const processo = spawn(servidorDaBancada, proprietario ? ["--proprietario", proprietario] : [], {
    stdio: ["pipe", "pipe", "inherit"],
  });
  return new Promise((ok, falha) => {
    readline.createInterface({ input: processo.stdout }).once("line", (linha) => ok({ processo, ...JSON.parse(linha) }));
    processo.once("exit", (codigo) => falha(new Error(`o servidor da bancada saiu com ${codigo}`)));
  });
}

/**
 * O `.wasm` servido. Na reversão, os nove nomes de cargo da lista de comandos
 * da sessão são trocados por outros do mesmo tamanho, nos bytes: o `.wasm`
 * deixa de atender os nove, como antes deste conserto.
 */
function bytesDoWasm() {
  const bytes = fs.readFileSync(wasm);
  if (!REVERTER) return bytes;
  const nomes = ["pedir_cargos", "explicar_permissoes", "prever_cargo", "criar_cargo", "editar_cargo", "mover_cargo", "apagar_cargo", "atribuir_cargo", "revogar_cargo"];
  let trocas = 0;
  for (const nome of nomes) {
    const de = Buffer.from(nome);
    const para = Buffer.from(`${nome.slice(0, -1)}#`);
    let i = bytes.indexOf(de);
    while (i >= 0) {
      para.copy(bytes, i);
      trocas += 1;
      i = bytes.indexOf(de, i + 1);
    }
  }
  console.error(`reversão: ${trocas} nomes de cargo trocados no .wasm servido`);
  return bytes;
}

function servirAPagina() {
  const doWasm = bytesDoWasm();
  const servidor = http.createServer((pedido, resposta) => {
    const arquivos = {
      "/rtc-web.js": [path.join(UI, "rtc-web.js"), "text/javascript"],
      "/sessao-web.js": [path.join(UI, "sessao-web.js"), "text/javascript"],
    };
    if (pedido.url === "/seele_web.wasm") {
      resposta.writeHead(200, { "Content-Type": "application/wasm" });
      resposta.end(doWasm);
      return;
    }
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

/** Na página: entra nos servidores e deixa a sessão à mão da bancada. */
async function entrarNaPagina({ links, apelido }) {
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
  const voz = { async soltar() {}, async tomar() {} };
  const registro = criarRegistroDeSessoesWeb({ instanciar, voz });
  window.__registro = registro;
  window.__eventos = [];
  const ids = [];
  for (const texto of links) {
    const { link } = ler(texto);
    // A chave pelo MESMO `generateKey` que o `nascerAChave` do produto chama:
    // a bancada o fixou neste contexto.
    const chave = await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]);
    const alvo = { endereco: link.alvo, alternativos: link.alternativos ?? [], impressao: link.impressao, bilhete: link.bilhete ?? null, chave };
    const { id } = await registro.entrar({ alvo, apelido, convite: link.convite ?? null, emitir: (e) => window.__eventos.push(e) });
    ids.push(id);
  }
  return { ids, chavesEntregues: window.__chavesEntreguesPelaBancada() };
}

/** Na página: um comando de cargos pela sessão `id`, como a ponte o manda. */
function naSessao([id, comando, args]) {
  const feito = window.__registro.sessao(id).atender(comando, args);
  return feito === null ? { desconhecido: true } : feito;
}

/** Na página: os avisos de cargos de uma sessão, e a marca do retrato dela. */
function avisosDeCargos(id) {
  const avisos = window.__eventos
    .filter((e) => e.sessao === id && e.evento && e.evento.CargosChanged)
    .map((e) => e.evento.CargosChanged.marca);
  const r = window.__registro.sessao(id).retrato();
  return { avisos, marca: r?.sala?.cargos_revision ?? r?.cargos_revision ?? null };
}

/** Na página: quem esta sessão é. */
function quemSou(id) {
  const r = window.__registro.sessao(id).retrato();
  return r?.me ?? r?.sala?.me ?? null;
}

async function ate(fazer, condicao, prazo = 8000) {
  const fim = Date.now() + prazo;
  let ultimo;
  while (Date.now() < fim) {
    ultimo = await fazer();
    if (condicao(ultimo)) return ultimo;
    await new Promise((r) => setTimeout(r, 100));
  }
  return ultimo;
}

(async () => {
  const pagina = await servirAPagina();
  const url = `http://127.0.0.1:${pagina.address().port}/`;
  const { chromium, webkit } = require(playwright);
  const browser = await (navegador === "webkit" ? webkit : chromium).launch();
  const servidores = [];
  const R = { casos: {}, reversao: REVERTER };
  try {
    // Duas pessoas, dois contextos: cada uma com a sua chave fixa.
    const abas = [];
    for (let i = 0; i < 2; i += 1) {
      const contexto = await browser.newContext();
      await fixarAIdentidade(contexto);
      const aba = await contexto.newPage();
      aba.on("console", (m) => {
        if (m.type() === "error") console.error(`[página ${i + 1}] ${m.text()}`);
      });
      await aba.goto(url);
      abas.push(aba);
    }
    const [daDona, daBia] = abas;
    const impressaoDaDona = await impressaoDaPagina(daDona);
    // Recarregar não troca a pessoa.
    await daDona.reload();
    const depoisDeRecarregar = await impressaoDaPagina(daDona);
    const S = await subirUmServidor(impressaoDaDona);
    const T = await subirUmServidor(null);
    servidores.push(S, T);

    const dona = await daDona.evaluate(entrarNaPagina, { links: [S.link, T.link], apelido: "dona" });
    const bia = await daBia.evaluate(entrarNaPagina, { links: [S.link], apelido: "bia" });
    const [donaEmS, donaEmT] = dona.ids;
    const [biaEmS] = bia.ids;
    const cmd = (aba, id, comando, args = {}) => aba.evaluate(naSessao, [id, comando, args]);
    const tela = (aba, id) =>
      ate(() => cmd(aba, id, "pedir_cargos"), (f) => f.desconhecido || (f.resposta && f.resposta.quadro && f.resposta.suas));

    // ------------------------------------------------ 1. a dona é a dona
    const emS = await tela(daDona, donaEmS);
    const emT = await tela(daDona, donaEmT);
    R.casos.dona = {
      impressaoDaDona,
      recarregadaIgual: depoisDeRecarregar === impressaoDaDona,
      declaradaEmS: S.proprietario,
      chavesEntregues: dona.chavesEntregues,
      emS: emS.resposta?.suas ?? emS,
      emT: emT.resposta?.suas ?? emT,
    };

    // ---------------------------- 2. o verbo vai ao servidor e volta pelo quadro
    const revisaoS = emS.resposta?.quadro?.revisao ?? 0;
    const revisaoT = emT.resposta?.quadro?.revisao ?? 0;
    const criou = await cmd(daDona, donaEmS, "criar_cargo", {
      revisao: revisaoS,
      nome: "Moderação",
      concede: [{ Produto: "ManageRoles" }, { Produto: "Kick" }],
      restringe: [],
    });
    const depoisS = await ate(
      () => cmd(daDona, donaEmS, "pedir_cargos"),
      (f) => f.desconhecido || (f.resposta?.quadro?.cargos ?? []).some((c) => c.nome === "Moderação"),
    );
    const cargoNovo = (depoisS.resposta?.quadro?.cargos ?? []).find((c) => c.nome === "Moderação") ?? null;
    const depoisT = await cmd(daDona, donaEmT, "pedir_cargos");
    R.casos.verbo = {
      criou,
      revisaoAntes: revisaoS,
      revisaoDepois: depoisS.resposta?.quadro?.revisao ?? null,
      cargoNovo,
      tAntes: revisaoT,
      tDepois: depoisT.resposta?.quadro?.revisao ?? null,
      tTemOCargo: (depoisT.resposta?.quadro?.cargos ?? []).some((c) => c.nome === "Moderação"),
    };

    // ------------------------------------------ 3. a prévia é a do servidor
    const idDaBia = await daBia.evaluate(quemSou, biaEmS);
    const revisao = depoisS.resposta?.quadro?.revisao ?? 0;
    const pedido = cargoNovo ? { Atribuir: { pessoa: idDaBia, cargo: cargoNovo.id } } : null;
    // A Bia tem de ser conhecida pela sessão da dona (o FORA DO AR ou o roster).
    await ate(
      () => cmd(daDona, donaEmS, "prever_cargo", { revisao, pedido }),
      (f) => f.desconhecido || f.resposta || (f.erro && !JSON.stringify(f.erro).includes("PessoaNaoExiste")),
    );
    const previa = await cmd(daDona, donaEmS, "prever_cargo", { revisao, pedido });
    const velha = await cmd(daDona, donaEmS, "prever_cargo", { revisao: revisao - 1, pedido });
    R.casos.previa = { idDaBia, previa, velha };

    // ------------------------------------------ 4. atribuir chega à outra pessoa
    const atribuiu = await cmd(daDona, donaEmS, "atribuir_cargo", { revisao, pessoa: idDaBia, cargo: cargoNovo?.id ?? 0 });
    const daBiaDepois = await ate(
      () => cmd(daBia, biaEmS, "pedir_cargos"),
      (f) => f.desconhecido || (f.resposta?.suas?.permissoes ?? []).includes("ManageRoles"),
    );
    R.casos.atribuir = { atribuiu, suasDaBia: daBiaDepois.resposta?.suas ?? daBiaDepois };

    // ------------------------------------------------ 5. a recusa é do servidor
    // A Bia agora gere cargos, mas não pode conceder o que não tem (Ban).
    const revisaoDaBia = daBiaDepois.resposta?.quadro?.revisao ?? 0;
    const pediu = await cmd(daBia, biaEmS, "criar_cargo", { revisao: revisaoDaBia, nome: "Banidores", concede: [{ Produto: "Ban" }], restringe: [] });
    const recusada = await ate(
      () => cmd(daBia, biaEmS, "pedir_cargos"),
      (f) => f.desconhecido || Boolean(f.resposta?.ultima_recusa),
    );
    R.casos.recusa = { pediu, ultimaRecusa: recusada.resposta?.ultima_recusa ?? null };

    // ---------------------------------------------- 7. o aviso da mudança
    R.casos.aviso = {
      emS: await daDona.evaluate(avisosDeCargos, donaEmS),
      emT: await daDona.evaluate(avisosDeCargos, donaEmT),
    };

    // ---------------------------------------------------- 6. por sessão
    const explicaS = await cmd(daDona, donaEmS, "explicar_permissoes", { pessoa: idDaBia });
    const explicaT = await cmd(daDona, donaEmT, "explicar_permissoes", { pessoa: idDaBia });
    const manage = (f) => (Array.isArray(f.resposta) ? f.resposta.find((p) => p.permissao?.Produto === "ManageRoles") ?? null : f);
    R.casos.porSessao = { emS: manage(explicaS), emT: manage(explicaT) };
    R.navegador = `${navegador} ${browser.version()}`;
  } finally {
    await browser.close();
    pagina.close();
    for (const s of servidores) s.processo.stdin.end();
  }
  const c = R.casos;
  R.veredito = {
    donaEADona:
      /^[0-9a-f]{64}$/.test(c.dona.impressaoDaDona ?? "") &&
      c.dona.recarregadaIgual &&
      c.dona.declaradaEmS === c.dona.impressaoDaDona &&
      c.dona.emS?.proprietario === true &&
      c.dona.emT?.proprietario === false,
    verboPeloServidor:
      c.verbo.criou?.resposta === null && Boolean(c.verbo.cargoNovo) && c.verbo.revisaoDepois > c.verbo.revisaoAntes && !c.verbo.tTemOCargo && c.verbo.tDepois === c.verbo.tAntes,
    previaDoServidor:
      Boolean(c.previa.previa?.resposta?.pessoas?.some((p) => p.pessoa === c.previa.idDaBia && (p.ganha ?? []).some((g) => g.Produto === "ManageRoles"))) &&
      c.previa.velha?.erro?.CargoRecusado?.motivo?.RevisaoVelha !== undefined,
    atribuirChega: c.atribuir.atribuiu?.resposta === null && (c.atribuir.suasDaBia?.permissoes ?? []).includes("ManageRoles"),
    recusaDoServidor: c.recusa.pediu?.resposta === null && c.recusa.ultimaRecusa !== null && JSON.stringify(c.recusa.ultimaRecusa).includes("NaoPodeConceder"),
    porSessao: c.porSessao.emS?.tem === true && c.porSessao.emT?.tem === false,
    avisoDaMudanca:
      (c.aviso?.emS?.avisos ?? []).length >= 2 &&
      c.aviso.emS.avisos[c.aviso.emS.avisos.length - 1] === c.aviso.emS.marca &&
      c.aviso.emS.avisos.every((marca, i, todas) => i === 0 || marca > todas[i - 1]) &&
      (c.aviso.emT?.avisos ?? []).every((marca) => marca <= (c.aviso.emT?.marca ?? 0)),
  };
  const texto = JSON.stringify(R, null, 2);
  console.log(texto);
  if (saidaJson) fs.writeFileSync(saidaJson, texto + "\n");
  const passou = Object.values(R.veredito).every(Boolean);
  if (REVERTER) {
    const reprovadas = Object.values(R.veredito).filter((v) => !v).length;
    console.error(`reversão: ${reprovadas} de ${Object.keys(R.veredito).length} provas reprovam`);
    process.exit(reprovadas === Object.keys(R.veredito).length ? 0 : 1);
  }
  process.exit(passou ? 0 : 1);
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
