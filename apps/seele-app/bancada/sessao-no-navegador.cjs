// A sessão pelo navegador **de ponta a ponta** (Tarefa 2.6 da F4): a página
// inteira (`ui/`) num Chromium de verdade, entrando num servidor de verdade
// pela WebTransport, sem nada de mentira no meio.
//
//     node apps/seele-app/bancada/sessao-no-navegador.cjs \
//       target/wasm32-unknown-unknown/release/seele_web.wasm \
//       target/debug/examples/servidor_da_bancada \
//       target/wasm32-wasip1/release/seele_motor_web.wasm
//
// O servidor é o `Daemon` do `seeled` (`crates/seele-server/examples/
// servidor_da_bancada.rs`), num endereço local; ele dá o link do navegador
// que o desktop daria, com os hashes dos certificados curtos. A página entra
// por ele como entra pelo COLAR UM LINK: `identidadeDaPagina()`, `abrir(link)`,
// `entrar(apelido)`. Daí em diante, só o `invoke` e o `listen` da casca.
//
// O que se prova:
// 1. **a entrada**: o `entrar` resolve `{ ok: true }` com a `Entrada` do
//    desktop (o retrato com o nome do servidor, o veredito do primeiro contato
//    conferido e a geração), e as etapas da chegada chegam à casca;
// 2. **a conversa**: abrir o canal, mandar, e a mensagem volta confirmada
//    pelo servidor (`estado: "Confirmada"`, com a chave do eco), depois de um
//    `MessagesChanged`; e a busca a acha;
// 3. **duas pessoas**: a segunda página entra, e a primeira a vê na sala
//    depois de um `RosterChanged`;
// 4. **a voz atravessa**: as duas entram na mesma sala de voz, o áudio abre
//    (`abertura_do_audio: "aberto"`), e os datagramas que uma manda chegam e
//    são decodificados na outra; e o volume escolhido para a primeira chega à
//    mistura da segunda, pela alça;
// 5. **os hashes ficam guardados**: o `HashesDoNavegador` do servidor chega à
//    identidade, que guarda as janelas com o servidor;
// 6. **o fim**: o `disconnect` fecha, e o `snapshot` volta `NotConnected`;
// 7. **um hash que não é do servidor não entra**: a entrada desiste com
//    `sem-rede`, e nada fica pendurado;
// 8. **a casca larga do PC** (D-web-3), pelos comandos do desktop: o link
//    colado no SERVIDORES (`ler_entrada`, `analisar_convite` e o `connect` com
//    o endereço), a aparência guardada nos conhecidos, os aceites de MODs pelo
//    endereço, a volta pela lista sem link (`Known`), o endereço sem link
//    recusado com a frase, o CANCELAR de quem espera, e o esquecer;
// 9. **as imagens**: uma foto grande escolhida no seletor desce a escada do
//    desktop até caber no teto do ícone, o retrato fica guardado e volta do
//    servidor como a imagem da pessoa, e sai quando é tirado;
// 10. **os anexos**: um arquivo escolhido sobe pelo fluxo próprio (`Sent`),
//     volta na conversa, a prévia o desenha pelo juiz do desktop, e salvar o
//     entrega ao download do navegador com os mesmos bytes;
// 11. **a prévia de um link**, pelas regras do desktop: o endereço que não é
//     http, o destino privado (o IP, o IPv4 dentro do IPv6 e o nome local), o
//     domínio sem consentimento e o site que não responde. O juiz das imagens
//     é o mesmo da prévia de anexo do caso 10;
// 12. **a tela**: a primeira página compartilha a tela (a captura falsa do
//     Chromium, com som), uma página nova entra na sala, assiste, recebe a
//     abertura, os quadros (com chave) e o som da tela na mistura, quem
//     compartilha se vê pelo espelho, e parar fecha a tela de quem assistia;
// 13. **a volta**: o transporte da primeira página cai por baixo da sessão; o
//     retrato diz a bateria (`InternalBattery`) e a conversa segue legível, a
//     mensagem mandada na bateria fica pendente, a sessão volta sozinha por
//     outro transporte (`Online`, a mesma conversa), a pendente sai de novo e
//     volta confirmada, a voz entra de novo e chega à outra página, e quem
//     assistia vê a primeira na sala;
// 14. **os MODs da sessão**: um segundo servidor exige o MOD de referência
//     (`testes/mod-de-referencia`); a página entra pela identidade, aceita os
//     MODs e chega à sessão pelo passo da tela (`depoisDeConectar`). O laço
//     dos MODs da casca acha o pacote ausente e o obtém do catálogo — um
//     catálogo de teste, assinado pela bancada no formato do minisign e
//     servido no endereço do catálogo, conferido pela chave que a `prova`
//     troca —, o pacote fica no armazém da página, conferido pelo hash, e o
//     MOD sobe no executor do navegador e fala com a metade de servidor dele.
//
// Sai com 1 no primeiro caso que falhar, dizendo qual.

const { spawn } = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");
const readline = require("node:readline");

const raiz = path.resolve(__dirname, "..");
const { chromium } = require("./playwright.cjs");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");
const [caminhoDoWasm, caminhoDoServidor, caminhoDoMotor] = process.argv.slice(2);
if (![caminhoDoWasm, caminhoDoServidor, caminhoDoMotor].every((caminho) => caminho && fs.existsSync(caminho))) {
  console.error(
    "uso: node apps/seele-app/bancada/sessao-no-navegador.cjs <seele_web.wasm> <servidor_da_bancada> <seele_motor_web.wasm>\n" +
      "  (cargo xtask web, que compila os dois .wasm;\n" +
      "   cargo build -p seele-server --example servidor_da_bancada)",
  );
  process.exit(2);
}

/**
 * **O MOD que o segundo servidor exige**: o de referência do repositório,
 * numa cópia feita na hora com uma operação a mais na metade de servidor —
 * `ler-anexo`, que autoriza ler pelo volume o anexo que a janela mandou. O de
 * referência não lê imagem pelo volume, e sem ela a leitura só se provaria
 * recusada. O vetor do repositório não muda.
 */
const PASTA_DO_MOD = (() => {
  const copia = fs.mkdtempSync(path.join(os.tmpdir(), "mod-da-bancada-"));
  fs.cpSync(path.join(raiz, "testes", "mod-de-referencia"), copia, { recursive: true });
  const servidor = path.join(copia, "servidor", "main.js");
  const fonte = fs.readFileSync(servidor, "utf8");
  const ancora = '  if (o_que.op !== "contar" && o_que.op !== "gravar") {';
  if (!fonte.includes(ancora)) throw new Error("a metade de servidor do MOD de referência mudou: a âncora da bancada sumiu");
  fs.writeFileSync(
    servidor,
    fonte.replace(ancora, `  if (o_que.op === "ler-anexo") return JSON.stringify({ ok: true, volume: "referencia/anexo" });\n${ancora}`),
  );
  return copia;
})();

/** Onde o catálogo mora: o `ENDERECO` do `seele_sessao::catalogo`. */
const ENDERECO_DO_CATALOGO = "https://mods.seele.app.br";

let terminou = false;
process.on("beforeExit", () => {
  if (!terminou) {
    console.error("sessao-no-navegador: não chegou ao fim — uma promessa ficou pendurada");
    process.exitCode = 1;
  }
});

const falhas = [];
const confere = (caso, condicao, detalhe) => {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
};

const TIPOS = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".json": "application/json",
  ".woff2": "font/woff2",
};

/** A `ui/` inteira com o `.wasm` ao lado, servida com os cabeçalhos da origem. */
function servirAPagina() {
  const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "sessao-no-navegador-"));
  fs.cpSync(path.join(raiz, "ui"), pasta, { recursive: true });
  fs.copyFileSync(caminhoDoWasm, path.join(pasta, "seele_web.wasm"));
  fs.copyFileSync(caminhoDoMotor, path.join(pasta, "seele_motor_web.wasm"));
  return http.createServer((pedido, resposta) => {
    const nome = decodeURIComponent(new URL(pedido.url, "http://x").pathname).replace(/^\/+/, "") || "index.html";
    const arquivo = path.join(pasta, nome);
    if (!arquivo.startsWith(pasta) || !fs.existsSync(arquivo) || fs.statSync(arquivo).isDirectory()) {
      resposta.writeHead(404).end();
      return;
    }
    resposta.writeHead(200, { ...cabecalhosDaOrigem(`/${nome}`), "content-type": TIPOS[path.extname(nome)] ?? "application/octet-stream" });
    resposta.end(fs.readFileSync(arquivo));
  });
}

/** O servidor de verdade, e a primeira linha que ele diz. */
function subirOServidor(argumentos = []) {
  const processo = spawn(caminhoDoServidor, argumentos, { stdio: ["pipe", "pipe", "inherit"] });
  return new Promise((resolver, rejeitar) => {
    const linhas = readline.createInterface({ input: processo.stdout });
    linhas.once("line", (linha) => resolver({ processo, ...JSON.parse(linha) }));
    processo.once("exit", (codigo) => rejeitar(new Error(`o servidor da bancada saiu com ${codigo} antes de dizer o link`)));
  });
}

/**
 * **Uma chave do minisign para a bancada**: o `.pub` e quem assina, no formato
 * que o indexador publica (o modo legado, `Ed`, que assina os bytes crus), com
 * o comentário confiável assinado junto, como o `minisign-verify` confere.
 */
function chaveDoMinisign() {
  const { publicKey, privateKey } = crypto.generateKeyPairSync("ed25519");
  const publica = Buffer.from(publicKey.export({ format: "jwk" }).x, "base64url");
  const numero = crypto.randomBytes(8);
  const pub = `untrusted comment: chave da bancada\n${Buffer.concat([Buffer.from("Ed"), numero, publica]).toString("base64")}\n`;
  const assinar = (dados) => {
    const assinatura = crypto.sign(null, dados, privateKey);
    const comentario = "assinado pela bancada";
    const global = crypto.sign(null, Buffer.concat([assinatura, Buffer.from(comentario)]), privateKey);
    return (
      `untrusted comment: assinatura da bancada\n${Buffer.concat([Buffer.from("Ed"), numero, assinatura]).toString("base64")}\n` +
      `trusted comment: ${comentario}\n${global.toString("base64")}\n`
    );
  };
  return { pub, assinar };
}

/** Os arquivos de um pacote, pelo caminho de dentro com `/`, como o instalador os junta. */
function arquivosDoPacote(pasta, dentro = "") {
  const arquivos = [];
  for (const entrada of fs.readdirSync(path.join(pasta, dentro), { withFileTypes: true })) {
    const relativo = dentro ? `${dentro}/${entrada.name}` : entrada.name;
    if (!dentro && entrada.name === "dados") continue;
    if (entrada.isDirectory()) arquivos.push(...arquivosDoPacote(pasta, relativo));
    else if (entrada.isFile()) arquivos.push(relativo);
  }
  return arquivos;
}

/**
 * **O catálogo de teste**, servido no endereço do catálogo para uma página:
 * o MOD exigido, na versão e com o hash que o servidor disse, os arquivos da
 * pasta, e as revogações vazias, tudo assinado pela chave da bancada.
 * Devolve o que a página pediu, o catálogo, e como trocar um caminho por outro
 * corpo (e desfazer), para as recusas.
 */
async function servirOCatalogo(contexto, chave, exigido) {
  const arquivos = arquivosDoPacote(PASTA_DO_MOD);
  const catalogo = Buffer.from(JSON.stringify({
    esquema: 1,
    gerado_em: Math.floor(Date.now() / 1000),
    mods: [{ id: exigido.id, titulo: "Referência", versoes: [{ versao: exigido.version, api: 6, hash: exigido.hash, arquivos }] }],
  }));
  const revogacoes = Buffer.from(JSON.stringify({ esquema: 1, mods: [] }));
  const servidos = new Map([
    ["/catalogo.json", catalogo],
    ["/catalogo.json.minisig", Buffer.from(chave.assinar(catalogo))],
    ["/revogacoes.json", revogacoes],
    ["/revogacoes.json.minisig", Buffer.from(chave.assinar(revogacoes))],
  ]);
  for (const relativo of arquivos) {
    servidos.set(`/mods/${exigido.id}/${exigido.version}/${relativo}`, fs.readFileSync(path.join(PASTA_DO_MOD, relativo)));
  }
  const pedidos = [];
  const trocados = new Map();
  await contexto.route(`${ENDERECO_DO_CATALOGO}/**`, (rota) => {
    const caminho = new URL(rota.request().url()).pathname;
    pedidos.push(caminho);
    const corpo = trocados.get(caminho) ?? servidos.get(caminho);
    return corpo
      ? rota.fulfill({ status: 200, body: corpo, headers: { "access-control-allow-origin": "*" } })
      : rota.fulfill({ status: 404, body: "" });
  });
  return {
    pedidos,
    catalogo,
    trocar: (caminho, corpo) => trocados.set(caminho, Buffer.from(corpo)),
    restaurar: () => trocados.clear(),
  };
}

/**
 * Um PNG de ruído opaco, menor que o lado do ícone: o primeiro degrau não o
 * reduz (reduzir faria a média do ruído, e a imagem caberia sem a escada), e
 * nenhum degrau de cor cheia cabe no teto. A escada tem de descer até achatar
 * as cores.
 */
function pngDeRuido(largura, altura) {
  const zlib = require("node:zlib");
  const crc32 = (bytes) => {
    let c = ~0;
    for (const byte of bytes) {
      c ^= byte;
      for (let k = 0; k < 8; k += 1) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
    }
    return ~c >>> 0;
  };
  const pedaco = (tipo, dados) => {
    const comTipo = Buffer.concat([Buffer.from(tipo, "ascii"), dados]);
    const tamanho = Buffer.alloc(4);
    tamanho.writeUInt32BE(dados.length);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(comTipo));
    return Buffer.concat([tamanho, comTipo, crc]);
  };
  const cabecalho = Buffer.alloc(13);
  cabecalho.writeUInt32BE(largura, 0);
  cabecalho.writeUInt32BE(altura, 4);
  cabecalho.set([8, 6, 0, 0, 0], 8);
  const linhas = Buffer.alloc((largura * 4 + 1) * altura);
  let semente = 12345;
  for (let i = 0; i < linhas.length; i += 1) {
    semente = (semente * 1103515245 + 12345) >>> 0;
    const coluna = i % (largura * 4 + 1);
    // O filtro da linha em zero, o alfa opaco, e as cores sorteadas.
    linhas[i] = coluna === 0 ? 0 : coluna % 4 === 0 ? 0xff : (semente >>> 16) & 0xff;
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pedaco("IHDR", cabecalho),
    pedaco("IDAT", zlib.deflateSync(linhas)),
    pedaco("IEND", Buffer.alloc(0)),
  ]);
}

/** Escolhe `arquivo` no próximo seletor que a página abrir, e chama o comando. */
async function escolherNoSeletor(aba, arquivo, comando) {
  const seletor = aba.waitForEvent("filechooser");
  const chamada = invocar(aba, comando, {});
  await (await seletor).setFiles(arquivo);
  return chamada;
}

/** Espera uma condição na página, com prazo; devolve o último valor lido. */
async function esperar(aba, funcao, arg, prazoMs = 10_000) {
  const fim = Date.now() + prazoMs;
  let ultimo;
  for (;;) {
    ultimo = await aba.evaluate(funcao, arg);
    if (ultimo && ultimo.pronto) return ultimo;
    if (Date.now() > fim) return ultimo;
    await new Promise((r) => setTimeout(r, 100));
  }
}

/** Uma página da casca, ouvindo os eventos desde a carga. */
async function abrirUmaPagina(navegador, endereco) {
  const contexto = await navegador.newContext({ permissions: ["microphone"], acceptDownloads: true });
  const aba = await contexto.newPage();
  const doConsole = [];
  aba.on("console", (mensagem) => doConsole.push(`${mensagem.type()}: ${mensagem.text()}`));
  aba.on("pageerror", (erro) => doConsole.push(`pageerror: ${erro.message}`));
  await aba.addInitScript(() => {
    window.eventosDaBancada = [];
  });
  await aba.goto(`${endereco}index.html`);
  await aba.evaluate(() =>
    window.__TAURI__.event.listen("seele://event", ({ payload }) => window.eventosDaBancada.push(payload)),
  );
  return { contexto, aba, doConsole };
}

/** Entra pela identidade da página, como o COLAR UM LINK. */
function entrarPeloLink(aba, link, apelido) {
  return aba.evaluate(
    async ({ link, apelido }) => {
      const identidade = await identidadeDaPagina();
      const lido = await identidade.abrir(link);
      const entrada = await identidade.entrar(apelido);
      return { lido, entrada };
    },
    { link, apelido },
  );
}

const invocar = (aba, comando, args) =>
  aba.evaluate(
    async ({ comando, args }) => {
      try {
        return { ok: await window.__TAURI__.core.invoke(comando, args) };
      } catch (erro) {
        return { erro };
      }
    },
    { comando, args },
  );

(async () => {
  const servidorDaPagina = servirAPagina();
  await new Promise((resolve) => servidorDaPagina.listen(0, "127.0.0.1", resolve));
  const endereco = `http://localhost:${servidorDaPagina.address().port}/`;
  const servidor = await subirOServidor();
  const navegador = await chromium().launch({
    args: ["--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream", "--autoplay-policy=no-user-gesture-required"],
  });
  const paginas = [];
  /** Os servidores além do primeiro, encerrados no fim. */
  const servidoresDaBancada = [];
  try {
    // ------------------------------------------------------------ 1. a entrada
    const a = await abrirUmaPagina(navegador, endereco);
    paginas.push(a);
    const { lido, entrada } = await entrarPeloLink(a.aba, servidor.link, "ana");
    confere("1", lido && lido.estado === "pronto", `o link não ficou pronto: ${JSON.stringify(lido)}`);
    confere("1", entrada && entrada.ok === true, `a entrada não deu certo: ${JSON.stringify(entrada)}`);
    const resposta = entrada?.resposta ?? {};
    confere("1", resposta.snapshot?.server === "Bancada", `o retrato não traz o nome do servidor: ${JSON.stringify(resposta.snapshot?.server)}`);
    confere(
      "1",
      resposta.veredito?.FirstContactVerified?.fingerprint === servidor.impressao,
      `o veredito não é o primeiro contato conferido: ${JSON.stringify(resposta.veredito)}`,
    );
    confere("1", resposta.geracao === 1, `a geração da primeira entrada: ${resposta.geracao}`);
    const etapas = await a.aba.evaluate(() =>
      window.eventosDaBancada.filter((e) => e && e.ConnectStageChanged).map((e) => JSON.stringify(e.ConnectStageChanged.stage)),
    );
    confere("1", etapas.some((e) => e.includes("Tentando")) && etapas.includes('"Dentro"'), `as etapas da chegada: ${etapas.join(", ")}`);
    if (falhas.length) throw new Error("a entrada falhou; o resto depende dela");

    // ------------------------------------------------------------ 2. a conversa
    const retrato = (await invocar(a.aba, "snapshot", {})).ok;
    const canal = retrato?.channels?.[0]?.id;
    confere("2", Number.isInteger(canal), `o servidor não trouxe canal: ${JSON.stringify(retrato?.channels)}`);
    confere("2", (await invocar(a.aba, "open_channel", { channel: canal })).ok === null, "o open_channel não respondeu null");
    const chave = (await invocar(a.aba, "send_message", { channel: canal, body: "  olá da bancada  " })).ok;
    confere("2", typeof chave === "string" && /^\d+$/.test(chave), `a chave da mensagem: ${JSON.stringify(chave)}`);
    const confirmada = await esperar(
      a.aba,
      async (chave) => {
        const lista = await window.__TAURI__.core.invoke("messages");
        const minha = lista.find((m) => m.client_message_id === chave);
        return { pronto: minha && minha.estado === "Confirmada", minha };
      },
      chave,
    );
    confere("2", confirmada.pronto, `a mensagem não voltou confirmada: ${JSON.stringify(confirmada.minha)}`);
    confere("2", confirmada.minha?.body === "olá da bancada", `o corpo não foi aparado: ${JSON.stringify(confirmada.minha?.body)}`);
    // Três no mínimo: o do abrir e o do mandar saem do comando; o do eco só sai
    // da mensagem do servidor dobrada — é ele que diz que a dobra fala.
    const conversaMudou = await a.aba.evaluate(() => window.eventosDaBancada.filter((e) => e === "MessagesChanged").length);
    confere("2", conversaMudou >= 3, `a casca ouviu ${conversaMudou} MessagesChanged (abrir, mandar, o eco)`);

    // A busca sobre a conversa: o termo em outra caixa acha a mensagem, com o
    // deslocamento em caracteres (o «á» de «olá» conta um), o cursor anda, e
    // limpar responde sem erro.
    const busca = (await invocar(a.aba, "buscar", { termo: "BANCADA" })).ok;
    confere("2", busca?.total === 1 && busca?.atual?.start === 7 && busca?.atual?.end === 14, `a busca: ${JSON.stringify(busca)}`);
    const andou = (await invocar(a.aba, "busca_andar", { adiante: true })).ok;
    confere("2", andou?.posicao === 1, `a busca deu a volta: ${JSON.stringify(andou)}`);
    confere("2", (await invocar(a.aba, "busca_limpar", {})).ok === null, "o busca_limpar não respondeu null");

    // ------------------------------------------------------------ 3. duas pessoas
    const antesDaB = await a.aba.evaluate(() => window.eventosDaBancada.length);
    const b = await abrirUmaPagina(navegador, endereco);
    paginas.push(b);
    const entradaDaB = await entrarPeloLink(b.aba, servidor.link, "bia");
    confere("3", entradaDaB.entrada?.ok === true, `a segunda entrada: ${JSON.stringify(entradaDaB.entrada)}`);
    const viu = await esperar(a.aba, async () => {
      const r = await window.__TAURI__.core.invoke("snapshot");
      return { pronto: r.presentes.some((p) => p.nickname === "bia"), presentes: r.presentes.map((p) => p.nickname) };
    });
    confere("3", viu.pronto, `a primeira página não viu a segunda: ${JSON.stringify(viu.presentes)}`);
    // E a casca foi avisada: o `RosterChanged` da chegada dela, que só a dobra
    // do `PersonPresent` produz.
    const avisada = await a.aba.evaluate((antes) => window.eventosDaBancada.slice(antes).includes("RosterChanged"), antesDaB);
    confere("3", avisada, "a primeira página não ouviu o RosterChanged da chegada da segunda");

    // ------------------------------------------------------------ 4. a voz
    const sala = retrato?.voice_rooms?.[0]?.id;
    for (const { aba } of [a, b]) {
      await invocar(aba, "set_voice_mode", { mode: "Open" });
      const entrou = await invocar(aba, "enter_voice_room", { voiceRoom: sala, senha: null });
      confere("4", entrou.ok === null, `o enter_voice_room: ${JSON.stringify(entrou)}`);
    }
    for (const [nome, { aba }] of [["a", a], ["b", b]]) {
      const aberto = await esperar(aba, async () => {
        const r = await window.__TAURI__.core.invoke("snapshot");
        return { pronto: r.abertura_do_audio === "aberto", abertura: r.abertura_do_audio };
      }, undefined, 15_000);
      confere("4", aberto.pronto, `o áudio da página ${nome} não abriu: ${aberto.abertura}`);
    }
    const chegou = await esperar(b.aba, async () => {
      const contas = await vozDaPonte.atual.contas();
      return { pronto: contas.pagina.decodificados > 10, pagina: contas.pagina };
    }, undefined, 15_000);
    const daA = await a.aba.evaluate(async () => (await vozDaPonte.atual.contas()).pagina);
    confere(
      "4",
      chegou.pronto,
      `a voz da primeira não chegou à segunda: a enviou ${daA.enviados}, b recebeu ${chegou.pagina?.chegados} e decodificou ${chegou.pagina?.decodificados}`,
    );

    // O volume de quem fala: escolhido pelo apelido, aplicado pela alça.
    const desconhecida = await invocar(b.aba, "set_volume", { nickname: "ninguem", percent: 50 });
    confere("4", desconhecida.erro === "UnknownPerson", `o volume de quem não está: ${JSON.stringify(desconhecida)}`);
    confere("4", (await invocar(b.aba, "set_volume", { nickname: "ANA", percent: 0 })).ok === null, "o set_volume não respondeu null");
    const calada = await esperar(b.aba, async () => {
      const contas = await vozDaPonte.atual.contas();
      const ganhos = Object.values(contas.controles.ganhos);
      return { pronto: ganhos.length === 1 && ganhos[0] === 0, ganhos: contas.controles.ganhos };
    });
    confere("4", calada.pronto, `o ganho da primeira não chegou à mistura da segunda: ${JSON.stringify(calada.ganhos)}`);
    const volumes = (await invocar(b.aba, "volumes_locais", {})).ok;
    confere("4", JSON.stringify(volumes) === '{"ANA":0}', `os volumes locais: ${JSON.stringify(volumes)}`);

    // ------------------------------------------------------------ 5. os hashes guardados
    const guardado = await esperar(a.aba, async (impressao) => {
      const registro = await new Promise((resolver, rejeitar) => {
        const pedido = indexedDB.open("seele-identidade", 1);
        pedido.onerror = () => rejeitar(pedido.error);
        pedido.onsuccess = () => {
          const banco = pedido.result;
          const ler = banco.transaction("servidores", "readonly").objectStore("servidores").get(impressao);
          ler.onsuccess = () => {
            banco.close();
            resolver(ler.result ?? null);
          };
          ler.onerror = () => rejeitar(ler.error);
        };
      });
      return { pronto: Boolean(registro && registro.janelas && registro.janelas.length > 4), janelas: registro?.janelas?.length ?? 0 };
    }, servidor.impressao);
    confere("5", guardado.pronto, `as janelas do servidor não ficaram guardadas: ${guardado.janelas}`);

    // ------------------------------------------------------------ 6. o fim
    confere("6", (await invocar(b.aba, "disconnect", {})).ok === null, "o disconnect não respondeu null");
    const depois = await invocar(b.aba, "snapshot", {});
    confere("6", depois.erro === "NotConnected", `o snapshot depois do disconnect: ${JSON.stringify(depois)}`);
    const saiu = await esperar(a.aba, async () => {
      const r = await window.__TAURI__.core.invoke("snapshot");
      return { pronto: !r.presentes.some((p) => p.nickname === "bia"), presentes: r.presentes.map((p) => p.nickname) };
    });
    confere("6", saiu.pronto, `a primeira página ainda vê a segunda: ${JSON.stringify(saiu.presentes)}`);

    // ------------------------------------------------------------ 7. o hash errado
    const c = await abrirUmaPagina(navegador, endereco);
    paginas.push(c);
    const errado = servidor.link.replace(/wh=[0-9a-f]{64}/, `wh=${"ab".repeat(32)}`).replace(/,[0-9a-f]{64}/g, "");
    const comHashErrado = await entrarPeloLink(c.aba, errado, "caio");
    confere("7", comHashErrado.entrada?.erro === "sem-rede", `a entrada com o hash errado: ${JSON.stringify(comHashErrado.entrada)}`);

    // ------------------------------------------------------------ 9. as imagens
    const imagem = path.join(os.tmpdir(), `sessao-no-navegador-${process.pid}.png`);
    fs.writeFileSync(imagem, pngDeRuido(250, 250));
    const icone = await escolherNoSeletor(a.aba, imagem, "preparar_icone_do_server");
    const bytes = Buffer.from(icone.ok ?? []);
    confere(
      "9",
      bytes.length > 0 && bytes.length <= 8192 && bytes.subarray(1, 4).toString() === "PNG" && bytes.readUInt32BE(16) <= 256,
      `o ícone preparado: ${bytes.length} bytes, lado ${bytes.length > 24 ? bytes.readUInt32BE(16) : "?"}`,
    );
    const retratoDaA = await escolherNoSeletor(a.aba, imagem, "escolher_minha_imagem");
    confere("9", retratoDaA.ok === true, `o escolher_minha_imagem: ${JSON.stringify(retratoDaA)}`);
    const meu = (await invocar(a.aba, "meu_retrato", {})).ok;
    confere("9", typeof meu === "string" && meu.startsWith("data:image/png;base64,"), `o meu_retrato: ${String(meu).slice(0, 40)}`);
    const subiu = await esperar(a.aba, async () => {
      const r = await window.__TAURI__.core.invoke("snapshot");
      const imagem = await window.__TAURI__.core.invoke("imagem_da_pessoa", { person: r.me });
      return { pronto: typeof imagem === "string" && imagem.startsWith("data:image/png"), revisao: r.person_icons_revision };
    });
    confere("9", subiu.pronto, `a imagem própria não voltou do servidor (revisão ${subiu.revisao})`);
    confere("9", (await invocar(a.aba, "tirar_minha_imagem", {})).ok === null, "o tirar_minha_imagem não respondeu null");
    confere("9", (await invocar(a.aba, "meu_retrato", {})).ok === null, "o retrato ficou guardado depois de tirado");
    fs.rmSync(imagem, { force: true });

    // ------------------------------------------------------------ 10. os anexos
    const arquivo = path.join(os.tmpdir(), `anexo-${process.pid}.png`);
    const original = pngDeRuido(16, 16);
    fs.writeFileSync(arquivo, original);
    const escolhido = (await escolherNoSeletor(a.aba, arquivo, "escolher_arquivo")).ok;
    confere("10", escolhido?.tipo === "image/png" && escolhido?.tamanho === original.length, `o arquivo escolhido: ${JSON.stringify(escolhido)}`);
    const antesDoEnvio = await a.aba.evaluate(() => window.eventosDaBancada.length);
    const id = (await invocar(a.aba, "enviar_anexo", { channel: canal, body: "o anexo", caminho: escolhido?.caminho, nome: "ruido.png", tipo: "image/png" })).ok;
    confere("10", Number.isInteger(id), `o número do anexo enviado: ${JSON.stringify(id)}`);
    const enviado = await esperar(a.aba, async ({ antes, id }) => {
      const transferencias = window.eventosDaBancada.slice(antes).filter((e) => e && e.TransferChanged).map((e) => e.TransferChanged.transfer);
      return { pronto: transferencias.some((t) => t.kind === "Sent" && t.client_message_id === id), transferencias };
    }, { antes: antesDoEnvio, id });
    confere("10", enviado.pronto, `o anexo não subiu: ${JSON.stringify(enviado.transferencias)}`);
    const naConversa = await esperar(a.aba, async (antes) => {
      const lista = await window.__TAURI__.core.invoke("messages");
      const com = lista.find((m) => m.attachment && m.attachment.file_name === "ruido.png");
      const transferencias = window.eventosDaBancada.slice(antes).filter((e) => e && (e.TransferChanged || e.NoticeRaised));
      return { pronto: Boolean(com), com, transferencias, ultimas: lista.slice(-2) };
    }, antesDoEnvio);
    confere("10", naConversa.pronto, `o anexo não voltou na conversa: ${JSON.stringify({ eventos: naConversa.transferencias, ultimas: naConversa.ultimas })}`);
    const anexo = naConversa.com?.attachment?.id;
    const previa = (await invocar(a.aba, "prever_anexo", { anexo })).ok;
    confere(
      "10",
      previa?.refusal === null && previa?.found === "image/png" && String(previa?.image).startsWith("data:image/png;base64,"),
      `a prévia do anexo: ${JSON.stringify({ ...previa, image: String(previa?.image).slice(0, 30) })}`,
    );
    const destino = (await invocar(a.aba, "destino_do_anexo", { anexo })).ok;
    confere("10", destino?.file_name === "ruido.png" && destino?.beside === "ruido (2).png", `o destino: ${JSON.stringify(destino)}`);
    const baixando = a.aba.waitForEvent("download");
    confere("10", (await invocar(a.aba, "salvar_anexo", { anexo })).ok === null, "o salvar_anexo não respondeu null");
    const baixado = await baixando;
    const salvo = fs.readFileSync(await baixado.path());
    confere("10", baixado.suggestedFilename() === "ruido.png" && Buffer.compare(salvo, original) === 0, `o arquivo salvo: ${baixado.suggestedFilename()}, ${salvo.length} bytes`);
    fs.rmSync(arquivo, { force: true });

    // ------------------------------------------------------------ 11. a prévia de link
    const previaDe = async (url) => (await invocar(a.aba, "previa_de_link", { url })).erro;
    confere("11", (await previaDe("javascript:alert(1)"))?.NaoEAberto !== undefined, "um endereço que não é http saiu");
    confere("11", (await previaDe("http://127.0.0.1:8/x.png"))?.DestinoPrivado === "127.0.0.1", "o destino privado foi buscado");
    confere("11", (await previaDe("http://[::ffff:10.0.0.1]/x.png"))?.DestinoPrivado !== undefined, "o IPv4 mapeado em IPv6 passou");
    confere("11", (await previaDe("https://roteador.local/x.png"))?.DestinoPrivado === "roteador.local", "o nome local foi buscado");
    confere(
      "11",
      (await previaDe("https://exemplo.invalid/x.png"))?.SemConsentimento?.dominio === "exemplo.invalid",
      "um domínio sem consentimento foi buscado",
    );
    await invocar(a.aba, "consentir_previa", { dominio: "exemplo.invalid" });
    confere("11", (await previaDe("https://exemplo.invalid/x.png"))?.NaoRespondeu !== undefined, "o site que não existe não deu NaoRespondeu");

    // ------------------------------------------------------------ 8. a casca larga
    const d = await abrirUmaPagina(navegador, endereco);
    paginas.push(d);
    const comoLink = (await invocar(d.aba, "ler_entrada", { texto: servidor.link })).ok;
    confere("8", comoLink?.Convite === servidor.link, `o link colado não foi lido como convite: ${JSON.stringify(comoLink)}`);
    const comoEndereco = (await invocar(d.aba, "ler_entrada", { texto: "  10.9.9.9:8383 " })).ok;
    confere("8", comoEndereco?.Endereco === "10.9.9.9:8383", `o endereço colado: ${JSON.stringify(comoEndereco)}`);
    const convite = (await invocar(d.aba, "analisar_convite", { link: servidor.link })).ok;
    confere("8", convite?.alvo === servidor.endereco && convite?.para_entrar === "Aqui", `o convite lido: ${JSON.stringify(convite)}`);
    const pelaCasca = await invocar(d.aba, "connect", { server: convite?.alvo, nickname: "dani", audio: true, joinSecret: convite?.token ?? null, tentativa: 1 });
    confere("8", pelaCasca.ok?.snapshot?.server === "Bancada", `o connect da casca larga pelo link colado: ${JSON.stringify(pelaCasca)}`);
    confere("8", (await invocar(d.aba, "lembrar_aparencia_do_servidor", {})).ok === null, "o lembrar_aparencia_do_servidor não respondeu null");
    const daLista = await esperar(d.aba, async (alvo) => {
      const lista = await window.__TAURI__.core.invoke("conhecidos");
      const este = lista.find((servidor) => servidor.alvo === alvo);
      return { pronto: este && este.apelido === "dani" && este.nome === "Bancada", este };
    }, servidor.endereco);
    confere("8", daLista.pronto, `o servidor não ficou nos conhecidos com o apelido e o nome: ${JSON.stringify(daLista.este)}`);
    // Os aceites de MODs, pelo endereço que a casca do desktop usa.
    const conjunto = "ab".repeat(32);
    confere("8", (await invocar(d.aba, "aceite_de_mods", { alvo: servidor.endereco })).ok === null, "havia aceite antes de aceitar");
    confere("8", (await invocar(d.aba, "aceitar_mods", { alvo: servidor.endereco, conjunto })).ok === null, "o aceitar_mods não respondeu null");
    confere("8", (await invocar(d.aba, "aceite_de_mods", { alvo: servidor.endereco })).ok === conjunto, "o aceite não ficou guardado");
    const aceites = (await invocar(d.aba, "aceites_de_mods", {})).ok;
    confere(
      "8",
      Array.isArray(aceites) && aceites.length === 1 && aceites[0].alvo === servidor.endereco && aceites[0].quando > 0,
      `a lista dos aceites: ${JSON.stringify(aceites)}`,
    );
    confere("8", (await invocar(d.aba, "esquecer_aceite_de_mods", { alvo: servidor.endereco })).ok === null, "o esquecer_aceite_de_mods não respondeu null");
    confere("8", (await invocar(d.aba, "aceite_de_mods", { alvo: servidor.endereco })).ok === null, "o aceite esquecido voltou");
    // De volta pela lista, sem link: o servidor deste aparelho, pela chave.
    await invocar(d.aba, "disconnect", {});
    const pelaLista = await invocar(d.aba, "connect", { server: servidor.endereco, nickname: "dani", audio: true, joinSecret: null, tentativa: 2 });
    confere("8", pelaLista.ok?.veredito === "Known", `a volta pela lista: ${JSON.stringify(pelaLista.ok?.veredito ?? pelaLista)}`);
    await invocar(d.aba, "disconnect", {});
    const sozinho = await invocar(d.aba, "connect", { server: "10.9.9.9:8383", nickname: "dani", audio: true, joinSecret: null, tentativa: 3 });
    confere("8", typeof sozinho.erro?.RecusaDaPonte?.frase === "string", `um endereço sem link: ${JSON.stringify(sozinho)}`);
    // O CANCELAR de quem espera: um link para um endereço que não responde.
    const mudo = servidor.link.replace(servidor.endereco, "10.255.255.1:8383");
    const conviteMudo = (await invocar(d.aba, "analisar_convite", { link: mudo })).ok;
    const cancelada = await d.aba.evaluate(async (alvo) => {
      const esperando = window.__TAURI__.core.invoke("connect", { server: alvo, nickname: "dani", audio: true, joinSecret: null, tentativa: 4 });
      await new Promise((r) => setTimeout(r, 300));
      const cancelou = await window.__TAURI__.core.invoke("cancelar_ligacao");
      const inicio = Date.now();
      try {
        await esperando;
        return { cancelou, desfecho: "entrou" };
      } catch (erro) {
        return { cancelou, desfecho: erro, ms: Date.now() - inicio };
      }
    }, conviteMudo?.alvo);
    confere("8", cancelada.cancelou === true && cancelada.desfecho === "Cancelada" && cancelada.ms < 2000, `o CANCELAR: ${JSON.stringify(cancelada)}`);
    confere("8", (await invocar(d.aba, "cancelar_ligacao", {})).ok === false, "o CANCELAR sem entrada em curso não respondeu false");
    // Uma entrada que falha na hora (a porta que o navegador nem tenta): a
    // recusa chega à casca, e nada fica sem dono no console.
    const recusado = servidor.link.replace(servidor.endereco, "127.0.0.1:9");
    const conviteRecusado = (await invocar(d.aba, "analisar_convite", { link: recusado })).ok;
    const naHora = await invocar(d.aba, "connect", { server: conviteRecusado?.alvo, nickname: "dani", audio: true, joinSecret: null, tentativa: 5 });
    confere("8", naHora.erro === "Unreachable", `a entrada que falha na hora: ${JSON.stringify(naHora)}`);
    await new Promise((r) => setTimeout(r, 500));
    // E esquecer tira o servidor da lista.
    confere("8", (await invocar(d.aba, "esquecer", { alvo: servidor.endereco })).ok === null, "o esquecer não respondeu null");
    const depoisDeEsquecer = (await invocar(d.aba, "conhecidos", {})).ok;
    confere("8", !depoisDeEsquecer.some((servidor2) => servidor2.alvo === servidor.endereco), `o servidor esquecido ficou: ${JSON.stringify(depoisDeEsquecer)}`);

    // ------------------------------------------------------------ 12. a tela
    const e = await abrirUmaPagina(navegador, endereco);
    paginas.push(e);
    confere("12", (await entrarPeloLink(e.aba, servidor.link, "eva")).entrada?.ok === true, "a página que assiste não entrou");
    await invocar(e.aba, "enter_voice_room", { voiceRoom: sala, senha: null });
    const podeCompartilhar = (await invocar(a.aba, "permissao_de_tela", {})).ok;
    confere("12", podeCompartilhar === "Concedida", `a permissão da tela: ${JSON.stringify(podeCompartilhar)}`);
    const limites = { banda_bps: null, altura_maxima: 720, quadros_maximos: 15, prioridade: "nitidez", com_som: true };
    // Contados desde antes de compartilhar: quem está na sala e ainda não
    // escolheu recebe a tela sem pedir, e a abertura pode chegar antes do
    // `assistir`.
    const antesDeCompartilhar = await e.aba.evaluate(() => window.eventosDaBancada.length);
    const compartilhou = await invocar(a.aba, "compartilhar_tela", { fonte: 1, limites });
    confere("12", compartilhou.ok === null, `o compartilhar_tela: ${JSON.stringify(compartilhou)}`);
    const transmitindo = await esperar(e.aba, async () => {
      const r = await window.__TAURI__.core.invoke("snapshot");
      return { pronto: r.transmissoes.length === 1, transmissoes: r.transmissoes };
    }, undefined, 15_000);
    confere("12", transmitindo.pronto, `a página que assiste não viu a transmissão: ${JSON.stringify(transmitindo.transmissoes)}`);
    const tela = transmitindo.transmissoes?.[0]?.tela ?? transmitindo.transmissoes?.[0]?.screen;
    confere("12", (await invocar(e.aba, "assistir", { tela, quero: true })).ok === null, "o assistir não respondeu null");
    const assistindo = await esperar(e.aba, async (antes) => {
      const eventos = window.eventosDaBancada.slice(antes);
      const abriu = eventos.find((evento) => evento && evento.ScreenOpened);
      const chaves = eventos.filter((evento) => evento && evento.ScreenFrame && evento.ScreenFrame.key).length;
      const quadros = eventos.filter((evento) => evento && evento.ScreenFrame).length;
      const som = vozDaPonte.atual ? vozDaPonte.atual.estadoDoSomDaTela().quadros : 0;
      return { pronto: Boolean(abriu) && chaves >= 1 && quadros >= 5 && som > 0, abriu: abriu?.ScreenOpened, chaves, quadros, som };
    }, antesDeCompartilhar, 20_000);
    confere(
      "12",
      assistindo.pronto,
      `a tela não chegou a quem assiste: aberta ${JSON.stringify(assistindo.abriu)}, ${assistindo.quadros} quadros (${assistindo.chaves} chaves), ${assistindo.som} quadros de som`,
    );
    const espelho = await a.aba.evaluate(() => window.eventosDaBancada.some((evento) => evento && evento.ScreenOpened));
    confere("12", espelho, "quem compartilha não viu a própria tela (o espelho)");
    confere("12", (await invocar(a.aba, "parar_de_compartilhar", {})).ok === null, "o parar_de_compartilhar não respondeu null");
    const fechou = await esperar(e.aba, async (antes) => {
      const eventos = window.eventosDaBancada.slice(antes);
      return { pronto: eventos.some((evento) => evento && evento.ScreenClosed) };
    }, antesDeCompartilhar);
    confere("12", fechou.pronto, "a tela não fechou para quem assistia");

    // ------------------------------------------------------------ 13. a volta
    const naBateria = await a.aba.evaluate(async (canal) => {
      const invoke = window.__TAURI__.core.invoke;
      const antes = window.eventosDaBancada.length;
      sessaoDaPonte.atual.simularAQueda();
      // A bateria acende antes da primeira tentativa terminar: o retrato e a
      // conversa são lidos ali, e a mensagem sai para a fila da volta.
      const fim = Date.now() + 5000;
      let retrato = await invoke("snapshot");
      while (retrato.link === "Online" && Date.now() < fim) {
        await new Promise((r) => setTimeout(r, 5));
        retrato = await invoke("snapshot");
      }
      const conversa = await invoke("messages");
      const chave = await invoke("send_message", { channel: canal, body: "mandada na bateria" });
      const pendente = (await invoke("messages")).find((m) => m.client_message_id === chave);
      return {
        link: retrato.link,
        conversa: conversa.length,
        chave,
        pendente: pendente?.estado,
        avisou: window.eventosDaBancada.slice(antes).includes("TelemetryChanged"),
      };
    }, canal);
    confere(
      "13",
      typeof naBateria.link?.InternalBattery?.remaining_seconds === "number" && naBateria.link.InternalBattery.remaining_seconds <= 300,
      `o retrato na queda não diz a bateria: ${JSON.stringify(naBateria.link)}`,
    );
    confere("13", naBateria.avisou, "a casca não ouviu o TelemetryChanged da queda");
    confere("13", naBateria.conversa >= 1, `a conversa não seguiu legível na bateria: ${naBateria.conversa} mensagens`);
    confere("13", typeof naBateria.chave === "string" && naBateria.pendente && naBateria.pendente !== "Confirmada", `a mensagem da bateria: ${JSON.stringify(naBateria)}`);
    const voltou = await esperar(a.aba, async (chave) => {
      const r = await window.__TAURI__.core.invoke("snapshot");
      const minha = (await window.__TAURI__.core.invoke("messages")).find((m) => m.client_message_id === chave);
      return {
        pronto: r.link === "Online" && minha?.estado === "Confirmada" && r.abertura_do_audio === "aberto",
        link: r.link,
        estado: minha?.estado,
        abertura: r.abertura_do_audio,
        voltas: sessaoDaPonte.atual.contas.voltas,
      };
    }, naBateria.chave, 20_000);
    confere(
      "13",
      voltou.pronto && voltou.voltas === 1,
      `a sessão não voltou inteira: enlace ${JSON.stringify(voltou.link)}, a pendente ${voltou.estado}, o áudio ${voltou.abertura}, ${voltou.voltas} volta(s)`,
    );
    // Contados depois da volta: só a voz que voltou faz o número subir.
    const daEAntes = await e.aba.evaluate(async () => (await vozDaPonte.atual.contas()).pagina.decodificados);
    const vozDeNovo = await esperar(e.aba, async (antes) => {
      const r = await window.__TAURI__.core.invoke("snapshot");
      const decodificados = (await vozDaPonte.atual.contas()).pagina.decodificados;
      return { pronto: r.presentes.some((p) => p.nickname === "ana") && decodificados > antes + 25, decodificados };
    }, daEAntes, 20_000);
    confere("13", vozDeNovo.pronto, `a voz da primeira não voltou à outra página: ${daEAntes} → ${vozDeNovo.decodificados} decodificados`);
    // **O enlace que responde fica de pé**: três `Ping` de rotina sem `Pong`
    // (o `KEEPALIVE` de 5 s, três vezes) poriam a sessão na bateria sozinha, e
    // a bancada inteira roda em menos que isso. A espera passa da conta, e
    // nenhuma tentativa nova pode ter havido.
    await new Promise((r) => setTimeout(r, 16_000));
    const deDepois = await a.aba.evaluate(async () => ({
      link: (await window.__TAURI__.core.invoke("snapshot")).link,
      tentativas: sessaoDaPonte.atual.contas.tentativasDeVolta,
    }));
    confere(
      "13",
      deDepois.link === "Online" && deDepois.tentativas === 1,
      `o enlace que responde não ficou de pé: ${JSON.stringify(deDepois.link)}, ${deDepois.tentativas} tentativa(s) de volta`,
    );

    // ------------------------------------------------------------ 14. os MODs da sessão
    const servidorComMod = await subirOServidor([PASTA_DO_MOD]);
    servidoresDaBancada.push(servidorComMod);
    const exigido = servidorComMod.mod;
    confere("14", exigido && exigido.id === "seele/referencia" && /^[0-9a-f]{64}$/.test(exigido.hash), `o servidor não exige o MOD: ${JSON.stringify(exigido)}`);
    const f = await abrirUmaPagina(navegador, endereco);
    paginas.push(f);
    const chaveDaBancada = chaveDoMinisign();
    const catalogoDaBancada = await servirOCatalogo(f.contexto, chaveDaBancada, exigido);
    const doCatalogo = catalogoDaBancada.pedidos;
    // A `prova` troca a chave que a página confere pela da bancada.
    await f.aba.evaluate(async (pub) => {
      const w = (await carregarOWasmDaPagina()).instancia.exports;
      const bytes = new TextEncoder().encode(pub);
      const p = w.alocar(bytes.length);
      new Uint8Array(w.memory.buffer).set(bytes, p);
      w.prova_chave_do_catalogo(p, bytes.length);
      w.liberar(p, bytes.length);
    }, chaveDaBancada.pub);
    // **As recusas, antes de entrar** (o catálogo não pede sessão): um arquivo
    // trocado no caminho muda o conjunto, e um catálogo assinado por outra
    // chave não é lido; nos dois, nada fica guardado.
    const pedido = { id: exigido.id, versao: exigido.version, uso: "obter" };
    catalogoDaBancada.trocar(`/mods/${exigido.id}/${exigido.version}/cliente/main.js`, "// trocado no caminho");
    const divergente = await invocar(f.aba, "instalar_mod_do_catalogo", pedido);
    catalogoDaBancada.restaurar();
    confere(
      "14",
      divergente.erro?.ConteudoDivergente?.esperado === exigido.hash,
      `o arquivo trocado no caminho: ${JSON.stringify(divergente)}`,
    );
    catalogoDaBancada.trocar("/catalogo.json.minisig", chaveDoMinisign().assinar(catalogoDaBancada.catalogo));
    const deOutraChave = await invocar(f.aba, "instalar_mod_do_catalogo", pedido);
    catalogoDaBancada.restaurar();
    confere("14", deOutraChave.erro === "AssinaturaDoCatalogoRecusada", `o catálogo de outra chave: ${JSON.stringify(deOutraChave)}`);
    const nadaGuardado = (await invocar(f.aba, "mods_instalados", {})).ok;
    confere("14", Array.isArray(nadaGuardado) && nadaGuardado.length === 0, `a recusa deixou pacote guardado: ${JSON.stringify(nadaGuardado)}`);
    const comMod = await f.aba.evaluate(async ({ link, alvo }) => {
      const identidade = await identidadeDaPagina();
      await identidade.abrir(link);
      const primeira = await identidade.entrar("fê");
      if (primeira.erro !== "mods-a-aceitar") return { primeira };
      const aceita = await identidade.aceitarOsMods(primeira.conjunto);
      if (aceita.ok) depoisDeConectar(aceita.resposta, alvo);
      return { primeira: { erro: primeira.erro, mods: primeira.mods.map((m) => m.id) }, aceita: { ok: aceita.ok, erro: aceita.erro } };
    }, { link: servidorComMod.link, alvo: servidorComMod.endereco });
    confere(
      "14",
      comMod.primeira?.erro === "mods-a-aceitar" && comMod.primeira.mods.includes(exigido.id) && comMod.aceita?.ok === true,
      `a entrada com o MOD exigido: ${JSON.stringify(comMod)}`,
    );
    const oModSubiu = await esperar(f.aba, async (id) => {
      const instancia = modsCarregados.get(id);
      const instalados = await window.__TAURI__.core.invoke("mods_instalados");
      return {
        pronto: Boolean(instancia) && instancia.estado === ESTADOS_DE_MOD.ativa && instancia.executor?.nome === "navegador",
        estado: instancia?.estado ?? null,
        executor: instancia?.executor?.nome ?? null,
        fase: estadoDosMods.get(id) ?? null,
        instalados: instalados.map((m) => ({ id: m.id, hash: m.hash, refused: m.refused })),
      };
    }, exigido.id, 20_000);
    confere(
      "14",
      oModSubiu.pronto,
      `o MOD não subiu na página: ${JSON.stringify({ estado: oModSubiu.estado, executor: oModSubiu.executor, fase: oModSubiu.fase, instalados: oModSubiu.instalados, catalogo: doCatalogo })}`,
    );
    confere(
      "14",
      oModSubiu.instalados?.some((m) => m.id === exigido.id && m.hash === exigido.hash && m.refused === null),
      `o pacote não ficou no armazém com o hash exigido: ${JSON.stringify(oModSubiu.instalados)}`,
    );
    confere(
      "14",
      ["/catalogo.json", "/catalogo.json.minisig", "/revogacoes.json", "/revogacoes.json.minisig"].every((caminho) => doCatalogo.includes(caminho)) &&
        doCatalogo.some((caminho) => caminho.startsWith(`/mods/${exigido.id}/`)),
      `a página não buscou o catálogo inteiro: ${JSON.stringify(doCatalogo)}`,
    );
    // **E ele fala com a metade de servidor dele**: o painel que o MOD monta
    // diz as `vezes` que o `contar` do servidor devolveu, e só monta depois
    // da resposta; sem ela, as vezes ficam em zero.
    const falou = await esperar(f.aba, (id) => {
      const textos = [...document.querySelectorAll(`[data-mod="${id}"]`)].map((no) => no.textContent);
      const vezes = textos.join(" ").match(/vezes: (\d+)/);
      return { pronto: Boolean(vezes) && Number(vezes[1]) >= 1, textos: textos.map((texto) => texto.slice(0, 120)) };
    }, exigido.id, 15_000);
    confere("14", falou.pronto, `o painel do MOD não diz a resposta da metade de servidor: ${JSON.stringify(falou.textos)}`);
    // **A mídia do pacote**: o painel declara o som do MOD, a casca o pede
    // (`midia_do_mod`) e lê os bytes (`som_do_mod`) para decodificar. O botão
    // vira PARAR pelo próprio MOD, com som ou sem: quem diz que o som
    // carregou é a figura da mídia, `pronta` (ou `tocando`), e `falhou` sem os
    // bytes (medido pela reversão).
    const tocou = await f.aba.evaluate(async (id) => {
      const botao = [...document.querySelectorAll(`[data-mod="${id}"] button`)].find((b) => b.textContent.trim() === "TOCAR");
      if (!botao) return { semBotao: true };
      botao.click();
      const fim = Date.now() + 10_000;
      for (;;) {
        const texto = [...document.querySelectorAll(`[data-mod="${id}"]`)].map((no) => no.textContent).join(" ");
        if (/PARAR/.test(texto) || /som: (recusada|falhou)/.test(texto) || Date.now() > fim) {
          return {
            parar: /PARAR/.test(texto),
            aviso: (texto.match(/som: \w+/) ?? [null])[0],
            midia: CHAMADAS_DA_PONTE.get("midia_do_mod") ?? 0,
            som: CHAMADAS_DA_PONTE.get("som_do_mod") ?? 0,
            aFazer: [...CONTAS_DA_PONTE].filter(([, contas]) => contas.aFazer > 0).map(([comando]) => comando),
            estados: [...document.querySelectorAll(`[data-mod="${id}"] [data-estado]`)].map((no) => `${no.tagName}:${no.dataset.estado}`),
          };
        }
        await new Promise((r) => setTimeout(r, 100));
      }
    }, exigido.id);
    confere(
      "14",
      tocou.parar && tocou.midia >= 1 && tocou.som >= 1 && tocou.aFazer.length === 0 &&
        tocou.estados.length === 1 && /^FIGURE:(pronta|tocando)$/.test(tocou.estados[0]),
      `o som do pacote não tocou: ${JSON.stringify(tocou)}`,
    );
    // **A mídia do servidor**, reduzida ao uso: uma capa maior que o teto
    // volta no teto (960 × 320), e um texto que não é base64 é recusado.
    const capa = pngDeRuido(1200, 400).toString("base64");
    const doServidor = await f.aba.evaluate(async ({ id, capa }) => {
      const invoke = window.__TAURI__.core.invoke;
      const reduzida = await invoke("midia_em_bytes", { geracao: geracaoDaSessao, id, base64: capa, uso: "capa" });
      let torta;
      try {
        await invoke("midia_em_bytes", { geracao: geracaoDaSessao, id, base64: "isto não é base64", uso: null });
      } catch (erro) {
        torta = erro;
      }
      return { papel: reduzida.papel, decodificados: reduzida.decodificados, png: reduzida.uri.startsWith("data:image/png;base64,"), torta };
    }, { id: exigido.id, capa });
    confere(
      "14",
      doServidor.papel === "imagem" && doServidor.png && doServidor.decodificados === 960 * 320 * 4,
      `a capa do servidor não voltou reduzida: ${JSON.stringify(doServidor)}`,
    );
    confere("14", doServidor.torta?.Recusado?.motivo === "nao-e-base64", `o texto que não é base64: ${JSON.stringify(doServidor.torta)}`);
    // **O seletor do MOD, o pedaço, o envio e a leitura pelo volume**: a
    // pessoa escolhe pelo botão do painel, o MOD lê o primeiro pedaço, manda o
    // arquivo ao volume da metade de servidor dele (que abre a porta), e a
    // imagem volta pelo volume, os mesmos bytes; a leitura que o MOD não
    // autoriza é recusada por ele.
    const imagemDoMod = path.join(os.tmpdir(), `sessao-no-navegador-mod-${process.pid}.png`);
    const bytesDaImagem = pngDeRuido(250, 250);
    fs.writeFileSync(imagemDoMod, bytesDaImagem);
    const apertar = (rotulo) =>
      f.aba.evaluate(({ id, rotulo }) => {
        const botao = [...document.querySelectorAll(`[data-mod="${id}"] button`)].find((b) => b.textContent.trim() === rotulo);
        botao?.click();
        return Boolean(botao);
      }, { id: exigido.id, rotulo });
    const seletorDoMod = f.aba.waitForEvent("filechooser");
    confere("14", await apertar("ESCOLHER ARQUIVO"), "o painel do MOD não tem o ESCOLHER ARQUIVO");
    await (await seletorDoMod).setFiles(imagemDoMod);
    const leuOPedaco = await esperar(f.aba, async (id) => {
      const texto = [...document.querySelectorAll(`[data-mod="${id}"]`)].map((no) => no.textContent).join(" ");
      const lido = texto.match(/escolhido: (\S+) · (\d+) bytes · lidos (\d+)/);
      return { pronto: Boolean(lido) && Number(lido[3]) > 0, lido: lido?.slice(1) ?? null };
    }, exigido.id);
    confere(
      "14",
      // O MOD conta os bytes pelo base64 com o enchimento (`lidos`), e
      // passa do pedaço em até dois.
      leuOPedaco.pronto && leuOPedaco.lido[0] === "image/png" && Number(leuOPedaco.lido[1]) === bytesDaImagem.length &&
        Number(leuOPedaco.lido[2]) - Math.min(65535, bytesDaImagem.length) >= 0 &&
        Number(leuOPedaco.lido[2]) - Math.min(65535, bytesDaImagem.length) <= 2,
      `o MOD não leu o primeiro pedaço do que foi escolhido: ${JSON.stringify(leuOPedaco.lido)}`,
    );
    confere("14", await apertar("ENVIAR AO SERVIDOR"), "o painel do MOD não tem o ENVIAR AO SERVIDOR");
    const enviou = await esperar(f.aba, async (id) => {
      const texto = [...document.querySelectorAll(`[data-mod="${id}"]`)].map((no) => no.textContent).join(" ");
      return { pronto: texto.includes("arquivo no volume do servidor") || /enviar:|não conseguiu/.test(texto), texto: texto.slice(0, 300), guardados: BYTES_DOS_ESCOLHIDOS.size };
    }, exigido.id, 15_000);
    confere(
      "14",
      enviou.texto.includes("arquivo no volume do servidor") && enviou.guardados === 0,
      `o arquivo não chegou ao volume do servidor (ou não foi consumido): ${JSON.stringify(enviou)}`,
    );
    const peloVolume = await f.aba.evaluate(async (id) => {
      const invoke = window.__TAURI__.core.invoke;
      const lida = await invoke("ler_imagem_mod", { geracao: geracaoDaSessao, id, channel: 0, payload: JSON.stringify({ op: "ler-anexo" }), uso: "capa" });
      let recusada;
      try {
        await invoke("ler_imagem_mod", { geracao: geracaoDaSessao, id, channel: 0, payload: JSON.stringify({ op: "contar" }), uso: null });
      } catch (erro) {
        recusada = erro;
      }
      return { uri: lida.uri, papel: lida.papel, decodificados: lida.decodificados, recusada };
    }, exigido.id);
    confere(
      "14",
      peloVolume.uri === `data:image/png;base64,${bytesDaImagem.toString("base64")}` && peloVolume.decodificados === 250 * 250 * 4,
      `a imagem não voltou pelo volume com os mesmos bytes: ${JSON.stringify({ papel: peloVolume.papel, decodificados: peloVolume.decodificados, inicio: peloVolume.uri?.slice(0, 40) })}`,
    );
    confere(
      "14",
      peloVolume.recusada?.Recusado?.motivo === "leitura recusada pelo MOD",
      `a leitura que o MOD não autoriza: ${JSON.stringify(peloVolume.recusada)}`,
    );
    // O código que o executor recebeu é o do pacote, com as capacidades da
    // API 6, e a geração morta é recusada.
    const codigo = await f.aba.evaluate(async ({ id, hash }) => {
      const invoke = window.__TAURI__.core.invoke;
      const vivo = await invoke("codigo_do_mod", { geracao: geracaoDaSessao, id, hash });
      let morta;
      try {
        await invoke("codigo_do_mod", { geracao: geracaoDaSessao + 1, id, hash });
      } catch (erro) {
        morta = erro;
      }
      return { tamanho: vivo.fonte.length, capacidades: vivo.capacidades, morta };
    }, exigido);
    confere(
      "14",
      codigo.tamanho === fs.readFileSync(path.join(PASTA_DO_MOD, "cliente", "main.js"), "utf8").length && codigo.capacidades.includes("superficies"),
      `o código do MOD: ${JSON.stringify({ tamanho: codigo.tamanho, capacidades: codigo.capacidades })}`,
    );
    confere("14", codigo.morta?.Recusado?.motivo === "sessao-encerrada", `a geração morta: ${JSON.stringify(codigo.morta)}`);
    // **A saída solta os escolhidos**: um arquivo escolhido e não mandado sai
    // da página com a sessão, como no desktop.
    const outroSeletor = f.aba.waitForEvent("filechooser");
    await apertar("ESCOLHER ARQUIVO");
    await (await outroSeletor).setFiles(imagemDoMod);
    const deNovo = await esperar(f.aba, () => ({ pronto: BYTES_DOS_ESCOLHIDOS.size === 1, guardados: BYTES_DOS_ESCOLHIDOS.size }));
    confere("14", deNovo.pronto, `o segundo arquivo escolhido não ficou guardado: ${deNovo.guardados}`);
    confere("14", (await invocar(f.aba, "disconnect", {})).ok === null, "o disconnect da página com o MOD não respondeu null");
    const soltos = await f.aba.evaluate(() => BYTES_DOS_ESCOLHIDOS.size);
    confere("14", soltos === 0, `a saída deixou ${soltos} arquivo(s) escolhido(s) na página`);

    for (const [nome, { doConsole }] of [["a", a], ["b", b], ["c", c], ["d", d], ["e", e], ["f", f]]) {
      const erros = doConsole.filter((linha) => linha.startsWith("pageerror") || /CSP|Content Security Policy/i.test(linha));
      confere("console", erros.length === 0, `a página ${nome} teve erros: ${erros.join(" | ")}`);
    }
  } catch (erro) {
    falhas.push(`a bancada parou: ${erro && erro.stack ? erro.stack : erro}`);
  } finally {
    if (falhas.length) {
      for (const [indice, { doConsole }] of paginas.entries()) {
        console.error(`--- console da página ${indice + 1}:\n  ${doConsole.slice(-25).join("\n  ")}`);
      }
    }
    await navegador.close();
    servidorDaPagina.close();
    servidor.processo.stdin.end();
    for (const outro of servidoresDaBancada) outro.processo.stdin.end();
    terminou = true;
  }
  if (falhas.length) {
    console.error(`sessao-no-navegador: ${falhas.length} falha(s)\n  ${falhas.join("\n  ")}`);
    process.exit(1);
  }
  console.log(
    "sessao-no-navegador: a página entra num servidor de verdade pela WebTransport, conversa, vê a outra pessoa, a voz atravessa, os hashes ficam guardados, sai, um hash errado não entra, a sessão que cai volta sozinha, e o MOD que o servidor exige vem do catálogo e sobe.",
  );
})();
