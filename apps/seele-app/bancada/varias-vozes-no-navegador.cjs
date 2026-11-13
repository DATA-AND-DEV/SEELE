// Várias vozes ao mesmo tempo, ouvidas por uma página (o relato de
// 13/11/2026, no teste real da 1.1.0: «um usuário que estava no navegador
// começou a escutar todos os áudios pipocados depois que alguém entrou no
// servidor junto»).
//
//     node apps/seele-app/bancada/varias-vozes-no-navegador.cjs \
//       target/wasm32-unknown-unknown/release/seele_web.wasm \
//       target/debug/examples/servidor_da_bancada \
//       target/wasm32-wasip1/release/seele_motor_web.wasm \
//       [--cpu <vezes>] [--segundos <n>] [--json <arquivo>]
//
// A página inteira (`ui/`) num Chromium de verdade, contra o servidor de
// verdade (`servidor_da_bancada`), pelo enlace WebRTC: nada de mentira entre o
// microfone de quem fala e o alto-falante de quem ouve, menos o próprio
// microfone. **Quem fala manda um tom contínuo**, cada pessoa o seu (470, 1130
// e 1870 Hz, nenhum deles com um número inteiro de ciclos em 20 ms: um quadro
// fora do lugar muda a fase), posto no lugar do microfone no mesmo
// `AudioContext` da voz, antes do worklet de captura — o portão, o relógio, o
// `AudioEncoder` e o datagrama são os do produto.
//
// **Quem ouve é medido em dois lugares**, em janelas de alguns segundos:
//
// - **nas contas do produto**: os datagramas que chegaram, os decodificados e
//   os que ficaram sem o cabeçalho (`voz-web.js`), e por fonte os quadros
//   tocados, ocultados (silêncio com fade, no navegador), silenciados,
//   atrasados, transbordados e as ressincronias do buffer de jitter, o alvo e a
//   profundidade dele (`Reproducao::contas`), mais a perda e o jitter que a
//   própria página mede (`voz_tocar_recepcao`);
// - **no som que saiu**: um gravador ligado à saída do worklet de reprodução
//   grava a mistura, e cada tom é procurado nela de 5 em 5 ms. `presença` é a
//   fração do tempo em que o tom está lá com a amplitude de origem, e
//   `estalos` conta as vezes em que ele some, muda de amplitude ou pula de
//   fase — o que o ouvido chama de pipocar.
//
// O roteiro é o do relato:
//
// 1. **uma voz**: quem ouve entra na sala, a primeira pessoa fala sem parar;
// 2. **duas vozes**: mais uma entra e fala junto;
// 3. **três vozes**: mais uma;
// 4. **o revezamento**: todas se calam, a primeira fala, cala, a segunda fala,
//    cala, e a primeira volta a falar — sem ninguém falar junto.
//
// O que se exige, em cada passo: nenhum quadro decodificado sem o cabeçalho
// dele, cada fonte com quase todos os quadros tocados e quase nenhum ocultado,
// atrasado ou ressincronizado, e cada tom presente e sem estalos na saída.
//
// `--cpu <vezes>` estrangula a CPU da página de quem ouve
// (`Emulation.setCPUThrottlingRate`), que é o que aproxima um celular ou um
// notebook fraco. `--json` grava os números.
//
// O que ela **não** prova: rede de verdade (o enlace é local, sem perda), um
// celular de verdade, nem o WebKit (o tom posto no lugar do microfone e o
// gravador são os mesmos lá, mas ninguém a rodou nele).
//
// Sai com 1 no primeiro caso que falhar, dizendo qual.

const { spawn } = require("node:child_process");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");
const readline = require("node:readline");

const raiz = path.resolve(__dirname, "..");
const { chromium } = require("./playwright.cjs");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");

const argumentos = process.argv.slice(2);
const opcao = (nome) => (argumentos.includes(nome) ? argumentos[argumentos.indexOf(nome) + 1] : null);
const COM_VALOR = ["--cpu", "--segundos", "--json"];
const posicionais = argumentos.filter((a, i) => !a.startsWith("--") && !(i > 0 && COM_VALOR.includes(argumentos[i - 1])));
const [caminhoDoWasm, caminhoDoServidor, caminhoDoMotor] = posicionais;
if (![caminhoDoWasm, caminhoDoServidor, caminhoDoMotor].every((caminho) => caminho && fs.existsSync(caminho))) {
  console.error(
    "uso: node apps/seele-app/bancada/varias-vozes-no-navegador.cjs <seele_web.wasm> <servidor_da_bancada> <seele_motor_web.wasm> [--cpu <vezes>] [--segundos <n>] [--json <arquivo>]\n" +
      "  (cargo xtask web, que compila os dois .wasm;\n" +
      "   cargo build -p seele-server --example servidor_da_bancada)",
  );
  process.exit(2);
}
const CPU = Number(opcao("--cpu") ?? 1);
const SEGUNDOS = Number(opcao("--segundos") ?? 5);
const SAIDA_JSON = opcao("--json");

/** O tom de cada pessoa que fala, em Hz, e a amplitude com que ele sai. */
const TONS = [470, 1130, 1870];
const AMPLITUDE = 0.25;
const QUADROS_POR_SEGUNDO = 50;

let terminou = false;
process.on("beforeExit", () => {
  if (!terminou) {
    console.error("varias-vozes-no-navegador: não chegou ao fim — uma promessa ficou pendurada");
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

/**
 * O gravador: um processador que guarda o que entra nele enquanto alguém
 * manda gravar. Mora na cópia servida, e nunca na `ui/`: um arquivo de bancada
 * esquecido lá iria para o instalador.
 */
const GRAVADOR = `"use strict";
class GravadorDaBancada extends AudioWorkletProcessor {
  constructor() {
    super();
    this.gravando = false;
    this.pedacos = [];
    this.port.onmessage = (evento) => {
      if (evento.data === "comecar") {
        this.pedacos = [];
        this.gravando = true;
      } else if (evento.data === "parar") {
        this.gravando = false;
        const som = new Float32Array(this.pedacos.length * 128);
        this.pedacos.forEach((pedaco, i) => som.set(pedaco, i * 128));
        this.pedacos = [];
        this.port.postMessage(som, [som.buffer]);
      }
    };
  }
  process(entradas) {
    if (!this.gravando) return true;
    const canal = entradas[0] && entradas[0][0];
    this.pedacos.push(canal && canal.length === 128 ? canal.slice() : new Float32Array(128));
    return true;
  }
}
registerProcessor("gravador-da-bancada", GravadorDaBancada);
`;

/** A `ui/` inteira com os `.wasm` e o gravador ao lado, com os cabeçalhos da origem. */
function servirAPagina() {
  const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "varias-vozes-no-navegador-"));
  fs.cpSync(path.join(raiz, "ui"), pasta, { recursive: true });
  fs.copyFileSync(caminhoDoWasm, path.join(pasta, "seele_web.wasm"));
  fs.copyFileSync(caminhoDoMotor, path.join(pasta, "seele_motor_web.wasm"));
  fs.writeFileSync(path.join(pasta, "gravador-da-bancada.js"), GRAVADOR);
  const servidor = http.createServer((pedido, resposta) => {
    const nome = decodeURIComponent(new URL(pedido.url, "http://x").pathname).replace(/^\/+/, "") || "index.html";
    const arquivo = path.join(pasta, nome);
    if (!arquivo.startsWith(pasta) || !fs.existsSync(arquivo) || fs.statSync(arquivo).isDirectory()) {
      resposta.writeHead(404).end();
      return;
    }
    resposta.writeHead(200, { ...cabecalhosDaOrigem(`/${nome}`), "content-type": TIPOS[path.extname(nome)] ?? "application/octet-stream" });
    resposta.end(fs.readFileSync(arquivo));
  });
  return { servidor, pasta };
}

/** O servidor de verdade, e a primeira linha que ele diz. */
function subirOServidor() {
  const processo = spawn(caminhoDoServidor, [], { stdio: ["pipe", "pipe", "inherit"] });
  return new Promise((resolver, rejeitar) => {
    const linhas = readline.createInterface({ input: processo.stdout });
    linhas.once("line", (linha) => resolver({ processo, ...JSON.parse(linha) }));
    processo.once("exit", (codigo) => rejeitar(new Error(`o servidor da bancada saiu com ${codigo} antes de dizer o link`)));
  });
}

const dormir = (ms) => new Promise((resolver) => setTimeout(resolver, ms));

/** Espera uma condição na página, com prazo; devolve o último valor lido. */
async function esperar(aba, funcao, arg, prazoMs = 15_000) {
  const fim = Date.now() + prazoMs;
  let ultimo;
  for (;;) {
    ultimo = await aba.evaluate(funcao, arg);
    if (ultimo && ultimo.pronto) return ultimo;
    if (Date.now() > fim) return ultimo;
    await dormir(100);
  }
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

/**
 * Uma página da casca. Com `tom`, **o microfone dela é um tom contínuo**: o
 * `createMediaStreamSource` da voz devolve um oscilador do mesmo contexto, e o
 * resto do caminho (o worklet de captura, o portão, o codec) é o do produto.
 * Sem `tom`, é a página de quem ouve: ela guarda os nós de worklet que a voz
 * criar, para o gravador se ligar à saída da reprodução.
 */
async function abrirUmaPagina(navegador, endereco, { tom = null } = {}) {
  const contexto = await navegador.newContext({ permissions: ["microphone"] });
  const aba = await contexto.newPage();
  // **Nada sai da máquina**: a página de produto procura o ponto de encontro
  // das chamadas pessoais, que não é assunto desta bancada. O pedido é
  // recusado aqui, e por isso só o erro que a página não pegou conta como
  // falha (a recusa aparece no console como um recurso que não carregou).
  await contexto.route(/^https?:\/\/(?!localhost[:/]|127\.0\.0\.1[:/])/, (rota) => rota.abort());
  const erros = [];
  aba.on("pageerror", (erro) => erros.push(`pageerror: ${erro.message}`));
  await aba.addInitScript(
    ({ tom, amplitude }) => {
      if (tom) {
        AudioContext.prototype.createMediaStreamSource = function () {
          const oscilador = this.createOscillator();
          oscilador.frequency.value = tom;
          const ganho = this.createGain();
          ganho.gain.value = amplitude;
          oscilador.connect(ganho);
          oscilador.start();
          return ganho;
        };
        return;
      }
      const DoNavegador = window.AudioWorkletNode;
      window.nosDaBancada = [];
      window.AudioWorkletNode = class extends DoNavegador {
        constructor(ctx, nome, opcoes) {
          super(ctx, nome, opcoes);
          window.nosDaBancada.push({ ctx, nome, no: this });
        }
      };
    },
    { tom, amplitude: AMPLITUDE },
  );
  await aba.goto(`${endereco}index.html`);
  return { contexto, aba, erros };
}

/** Entra pela identidade da página, como o COLAR UM LINK. */
function entrarPeloLink(aba, link, apelido) {
  return aba.evaluate(
    async ({ link, apelido }) => {
      const identidade = await identidadeDaPagina();
      await identidade.abrir(link);
      return identidade.entrar(apelido);
    },
    { link, apelido },
  );
}

/** Entra na sala de voz e espera o áudio abrir. */
async function entrarNaVoz(pagina, nome, sala, modo) {
  if (modo) await invocar(pagina.aba, "set_voice_mode", { mode: modo });
  const entrou = await invocar(pagina.aba, "enter_voice_room", { voiceRoom: sala, senha: null });
  confere("a sala de voz", entrou.ok === null, `o enter_voice_room de ${nome}: ${JSON.stringify(entrou)}`);
  const aberto = await esperar(pagina.aba, async () => {
    const r = await window.__TAURI__.core.invoke("snapshot");
    return { pronto: r.abertura_do_audio === "aberto", abertura: r.abertura_do_audio };
  });
  confere("a sala de voz", aberto.pronto, `o áudio de ${nome} não abriu: ${aberto.abertura}`);
}

/** Liga o gravador à saída do worklet de reprodução de quem ouve. */
function ligarOGravador(aba) {
  return aba.evaluate(async () => {
    const achado = window.nosDaBancada.find((n) => n.nome === "seele-reproducao-da-voz");
    if (!achado) return "a voz não criou o worklet de reprodução";
    const { ctx, no } = achado;
    await ctx.audioWorklet.addModule("gravador-da-bancada.js");
    const gravador = new AudioWorkletNode(ctx, "gravador-da-bancada", { numberOfInputs: 1, numberOfOutputs: 1, channelCount: 1, channelCountMode: "explicit" });
    no.connect(gravador);
    // Um nó sem saída ligada pode não ser processado; o ganho zero o mantém puxado.
    const calado = ctx.createGain();
    calado.gain.value = 0;
    gravador.connect(calado).connect(ctx.destination);
    window.gravadorDaBancada = gravador;
    return null;
  });
}

/** As contas de quem ouve, agora. */
function lerAsContas(aba) {
  return aba.evaluate(async () => {
    const contas = await vozDaPonte.atual.contas();
    return {
      pagina: {
        chegados: contas.pagina.chegados,
        decodificados: contas.pagina.decodificados,
        semCabecalho: contas.pagina.semCabecalho,
        tortos: contas.pagina.tortos,
        decodificadoresAbertos: contas.pagina.decodificadoresAbertos,
        falhas: contas.pagina.falhas.length,
      },
      fontes: contas.reproducao.contas.fontes,
      fades: contas.reproducao.contas.fades,
      mistura: contas.reproducao.contas.mistura,
      falhasDaMistura: contas.reproducao.falhasDaMistura,
      recepcao: { ...vozDaSessao.recepcao },
    };
  });
}

/**
 * Grava `segundos` da saída de quem ouve e procura cada tom nela.
 *
 * Cada tom é medido de 5 em 5 ms, numa janela de Hann de 20 ms, contra uma
 * referência que conta as amostras desde o começo da gravação: um tom contínuo
 * dá a mesma amplitude e a mesma fase em todas as janelas. Uma janela está
 * **perturbada** quando a amplitude sai de 70% a 130% da de origem ou a fase
 * anda mais de 0,35 rad desde a anterior; um **estalo** é um trecho de janelas
 * perturbadas (dois trechos a menos de 20 ms um do outro contam como um).
 */
function gravarEMedir(aba, segundos, tons) {
  return aba.evaluate(
    async ({ segundos, tons, amplitude }) => {
      const gravador = window.gravadorDaBancada;
      const som = await new Promise((resolver) => {
        gravador.port.onmessage = (evento) => resolver(evento.data);
        gravador.port.postMessage("comecar");
        setTimeout(() => gravador.port.postMessage("parar"), segundos * 1000);
      });
      const JANELA = 960;
      const PASSO = 240;
      const hann = Float64Array.from({ length: JANELA }, (_, i) => 0.5 - 0.5 * Math.cos((2 * Math.PI * i) / JANELA));
      const soma = hann.reduce((a, b) => a + b, 0);
      const medidos = {};
      for (const tom of tons) {
        const w = (2 * Math.PI * tom) / 48000;
        const amplitudes = [];
        const fases = [];
        for (let inicio = 0; inicio + JANELA <= som.length; inicio += PASSO) {
          let re = 0;
          let im = 0;
          for (let i = 0; i < JANELA; i++) {
            const x = som[inicio + i] * hann[i];
            re += x * Math.cos(w * (inicio + i));
            im -= x * Math.sin(w * (inicio + i));
          }
          amplitudes.push((2 * Math.hypot(re, im)) / soma);
          fases.push(Math.atan2(im, re));
        }
        let presentes = 0;
        let perturbadas = 0;
        let estalos = 0;
        let desdeAUltima = Infinity;
        for (let n = 0; n < amplitudes.length; n++) {
          const noLugar = amplitudes[n] >= 0.7 * amplitude && amplitudes[n] <= 1.3 * amplitude;
          if (noLugar) presentes++;
          let salto = 0;
          if (n > 0) {
            salto = Math.abs(fases[n] - fases[n - 1]);
            if (salto > Math.PI) salto = 2 * Math.PI - salto;
          }
          if (!noLugar || salto > 0.35) {
            perturbadas++;
            if (desdeAUltima > 4) estalos++;
            desdeAUltima = 0;
          } else {
            desdeAUltima++;
          }
        }
        const ordenadas = [...amplitudes].sort((a, b) => a - b);
        medidos[tom] = {
          presenca: amplitudes.length ? presentes / amplitudes.length : 0,
          estalos,
          perturbado_ms: perturbadas * 5,
          amplitude_mediana: ordenadas.length ? ordenadas[Math.floor(ordenadas.length / 2)] : 0,
        };
      }
      let pico = 0;
      for (let i = 0; i < som.length; i++) pico = Math.max(pico, Math.abs(som[i]));
      return { gravado_ms: Math.round(som.length / 48), pico, tons: medidos };
    },
    { segundos, tons, amplitude: AMPLITUDE },
  );
}

/** O que andou entre duas leituras das contas, por fonte e na página. */
function diferenca(antes, depois) {
  const pagina = {};
  for (const nome of ["chegados", "decodificados", "semCabecalho", "tortos", "falhas"]) pagina[nome] = depois.pagina[nome] - antes.pagina[nome];
  const fontes = {};
  for (const [alca, d] of Object.entries(depois.fontes)) {
    const a = antes.fontes[alca] ?? {};
    const andou = {};
    for (const nome of ["recebidos", "tocados", "ocultados", "silenciados", "calados", "atrasados", "transbordados", "repetidos", "ressincronias"]) andou[nome] = d[nome] - (a[nome] ?? 0);
    fontes[alca] = { ...andou, alvo_ms: d.alvo_ms, profundidade_ms: d.profundidade_ms, oscilacao_ms: d.oscilacao_ms };
  }
  return {
    pagina,
    fontes,
    decodificadoresAbertos: depois.pagina.decodificadoresAbertos,
    fades: depois.fades - antes.fades,
    falhasDaMistura: depois.falhasDaMistura - antes.falhasDaMistura,
    cortadas: depois.mistura.cortadas - antes.mistura.cortadas,
    recepcao: { perda: depois.recepcao.perda, jitter_ms: depois.recepcao.jitterMs },
  };
}

/** Uma janela de medida em quem ouve: as contas antes e depois, e o som no meio. */
async function medir(aba, tons, segundos = SEGUNDOS) {
  const antes = await lerAsContas(aba);
  const som = await gravarEMedir(aba, segundos, tons);
  const depois = await lerAsContas(aba);
  return { ...diferenca(antes, depois), som };
}

/** Uma linha por fonte e por tom, para quem lê a saída. */
function dizer(nome, m) {
  const fontes = Object.entries(m.fontes)
    .map(([alca, f]) => `    alça ${alca}: recebidos ${f.recebidos}, tocados ${f.tocados}, ocultados ${f.ocultados}, silenciados ${f.silenciados}, atrasados ${f.atrasados}, transbordados ${f.transbordados}, ressincronias ${f.ressincronias}, alvo ${Math.round(f.alvo_ms)} ms, profundidade ${Math.round(f.profundidade_ms)} ms`)
    .join("\n");
  const tons = Object.entries(m.som.tons)
    .map(([tom, t]) => `    ${tom} Hz: presença ${(t.presenca * 100).toFixed(1)}%, ${t.estalos} estalos (${t.perturbado_ms} ms perturbados), amplitude ${t.amplitude_mediana.toFixed(3)}`)
    .join("\n");
  console.log(
    `  ${nome} (${m.som.gravado_ms} ms gravados): chegaram ${m.pagina.chegados}, decodificados ${m.pagina.decodificados}, SEM CABEÇALHO ${m.pagina.semCabecalho}, ` +
      `fades ${m.fades}, a página mede perda ${(m.recepcao.perda * 100).toFixed(1)}% e jitter ${m.recepcao.jitter_ms.toFixed(1)} ms\n${fontes}\n${tons}`,
  );
}

/**
 * O que se exige de uma janela com `falantes` pessoas falando sem parar, cada
 * uma com o tom dela.
 *
 * As folgas são as de uma máquina de CI ocupada, e não as do defeito: com o
 * defeito, metade dos quadros fica sem o cabeçalho e os tons somem ou estalam
 * dezenas de vezes por segundo.
 */
function exigir(caso, m, tons, segundos = SEGUNDOS) {
  const esperados = segundos * QUADROS_POR_SEGUNDO;
  confere(caso, m.pagina.semCabecalho === 0, `${m.pagina.semCabecalho} quadros decodificados ficaram sem o cabeçalho deles (de ${m.pagina.decodificados + m.pagina.semCabecalho})`);
  confere(caso, m.pagina.falhas === 0 && m.falhasDaMistura === 0, `a voz disse ${m.pagina.falhas} falhas e a mistura ${m.falhasDaMistura}`);
  confere(caso, m.decodificadoresAbertos >= tons.length, `${m.decodificadoresAbertos} decodificadores abertos para ${tons.length} vozes`);
  const fontes = Object.values(m.fontes).filter((f) => f.recebidos > 0);
  confere(caso, fontes.length === tons.length, `${fontes.length} fontes receberam quadros, e falavam ${tons.length}: ${JSON.stringify(m.fontes)}`);
  for (const [alca, f] of Object.entries(m.fontes)) {
    if (f.recebidos === 0) continue;
    confere(caso, f.tocados >= 0.9 * esperados, `a alça ${alca} tocou ${f.tocados} quadros de uns ${esperados}: ${JSON.stringify(f)}`);
    confere(caso, f.ocultados + f.silenciados <= 0.04 * esperados, `a alça ${alca} teve ${f.ocultados} quadros ocultados e ${f.silenciados} silenciados em uns ${esperados}: ${JSON.stringify(f)}`);
    confere(caso, f.atrasados + f.transbordados <= 0.04 * esperados, `a alça ${alca} jogou fora ${f.atrasados} quadros atrasados e ${f.transbordados} transbordados em uns ${esperados}: ${JSON.stringify(f)}`);
    confere(caso, f.ressincronias === 0, `o buffer da alça ${alca} ressincronizou ${f.ressincronias} vezes`);
  }
  for (const tom of tons) {
    const t = m.som.tons[tom];
    confere(caso, t.presenca >= 0.9, `o tom de ${tom} Hz esteve na saída em ${(t.presenca * 100).toFixed(1)}% do tempo: ${JSON.stringify(t)}`);
    confere(caso, t.estalos <= 2 * segundos, `o tom de ${tom} Hz estalou ${t.estalos} vezes em ${segundos} s: ${JSON.stringify(t)}`);
  }
}

(async () => {
  const { servidor: servidorDaPagina, pasta } = servirAPagina();
  await new Promise((resolver) => servidorDaPagina.listen(0, "127.0.0.1", resolver));
  const endereco = `http://localhost:${servidorDaPagina.address().port}/`;
  const servidor = await subirOServidor();
  const navegador = await chromium().launch({
    args: ["--mute-audio", "--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream", "--autoplay-policy=no-user-gesture-required"],
  });
  const paginas = [];
  const medidas = { cpu: CPU, segundos: SEGUNDOS };
  try {
    // Quem ouve: entra, e fica no aperte-para-falar, sem falar.
    const ouvinte = await abrirUmaPagina(navegador, endereco);
    paginas.push(ouvinte);
    const entrada = await entrarPeloLink(ouvinte.aba, servidor.link, "ouve");
    confere("a entrada", entrada && entrada.ok === true, `quem ouve não entrou: ${JSON.stringify(entrada)}`);
    if (falhas.length) throw new Error("a entrada falhou; o resto depende dela");
    const sala = (await invocar(ouvinte.aba, "snapshot", {})).ok?.voice_rooms?.[0]?.id;
    confere("a entrada", Number.isInteger(sala), "o servidor não trouxe sala de voz");
    await entrarNaVoz(ouvinte, "quem ouve", sala, null);
    const semGravador = await ligarOGravador(ouvinte.aba);
    confere("o gravador", semGravador === null, String(semGravador));
    if (CPU > 1) {
      const cdp = await ouvinte.contexto.newCDPSession(ouvinte.aba);
      await cdp.send("Emulation.setCPUThrottlingRate", { rate: CPU });
    }

    /** Mais uma pessoa entra na sala e fala sem parar, com o tom dela. */
    const falantes = [];
    const maisUma = async () => {
      const tom = TONS[falantes.length];
      const pagina = await abrirUmaPagina(navegador, endereco, { tom });
      paginas.push(pagina);
      const entrou = await entrarPeloLink(pagina.aba, servidor.link, `fala${falantes.length + 1}`);
      confere("a entrada", entrou && entrou.ok === true, `a pessoa do tom de ${tom} Hz não entrou: ${JSON.stringify(entrou)}`);
      await entrarNaVoz(pagina, `a pessoa do tom de ${tom} Hz`, sala, "Open");
      falantes.push({ ...pagina, tom });
      const quantas = falantes.length;
      // A voz dela chegou a quem ouve: um decodificador por voz.
      const chegou = await esperar(ouvinte.aba, async (quantas) => {
        const contas = await vozDaPonte.atual.contas();
        return { pronto: contas.pagina.decodificadoresAbertos >= quantas, abertos: contas.pagina.decodificadoresAbertos, chegados: contas.pagina.chegados };
      }, quantas);
      confere("a entrada", chegou.pronto, `a voz de ${tom} Hz não chegou a quem ouve: ${JSON.stringify(chegou)}`);
      // O buffer de jitter da voz nova assenta antes da medida.
      await dormir(2000);
    };
    const calar = (pagina, on) => invocar(pagina.aba, "set_muted", { on });

    // ------------------------------------------------------------ 1. uma voz
    await maisUma();
    if (falhas.length) throw new Error("a primeira voz não chegou; o resto depende dela");
    medidas.uma = await medir(ouvinte.aba, TONS.slice(0, 1));
    dizer("uma voz", medidas.uma);
    exigir("1. uma voz", medidas.uma, TONS.slice(0, 1));

    // ---------------------------------------------------------- 2. duas vozes
    await maisUma();
    medidas.duas = await medir(ouvinte.aba, TONS.slice(0, 2));
    dizer("duas vozes", medidas.duas);
    exigir("2. duas vozes", medidas.duas, TONS.slice(0, 2));

    // ---------------------------------------------------------- 3. três vozes
    await maisUma();
    medidas.tres = await medir(ouvinte.aba, TONS.slice(0, 3));
    dizer("três vozes", medidas.tres);
    exigir("3. três vozes", medidas.tres, TONS.slice(0, 3));

    // -------------------------------------------------------- 4. o revezamento
    // Ninguém fala junto: a primeira, depois a segunda, depois a primeira de
    // novo, que é quem se mede. Os decodificadores das três seguem abertos (o
    // silêncio que faz esquecer alguém é de meio minuto).
    for (const pagina of falantes) await calar(pagina, true);
    await dormir(1000);
    await calar(falantes[0], false);
    await dormir(1500);
    await calar(falantes[0], true);
    await calar(falantes[1], false);
    await dormir(1500);
    await calar(falantes[1], true);
    await calar(falantes[0], false);
    await dormir(1500);
    const SEGUNDOS_DA_VOLTA = 3;
    medidas.volta = await medir(ouvinte.aba, TONS.slice(0, 1), SEGUNDOS_DA_VOLTA);
    dizer("a primeira de novo, depois da segunda", medidas.volta);
    exigir("4. o revezamento", medidas.volta, TONS.slice(0, 1), SEGUNDOS_DA_VOLTA);

    for (const pagina of paginas) {
      confere("a página", pagina.erros.length === 0, `erros que a página não pegou: ${JSON.stringify(pagina.erros.slice(0, 5))}`);
    }
    if (SAIDA_JSON) fs.writeFileSync(SAIDA_JSON, JSON.stringify(medidas, null, 2));
    if (!falhas.length) {
      const estalos = ["uma", "duas", "tres", "volta"].map((nome) => Object.values(medidas[nome].som.tons).reduce((soma, t) => soma + t.estalos, 0));
      console.log(
        `varias-vozes-no-navegador: quem ouve pela página ouviu uma, duas e três vozes ao mesmo tempo e a primeira de novo depois do revezamento, ` +
          `sem nenhum quadro sem cabeçalho e com ${estalos.join(", ")} estalos nos tons${CPU > 1 ? `, com a CPU dela ${CPU}× mais lenta` : ""}.`,
      );
    }
  } finally {
    await navegador.close();
    servidorDaPagina.close();
    servidor.processo.kill();
    fs.rmSync(pasta, { recursive: true, force: true });
  }
})()
  .catch((erro) => falhas.push(`a bancada quebrou: ${erro && erro.stack ? erro.stack : erro}`))
  .finally(() => {
    terminou = true;
    if (falhas.length) {
      for (const f of falhas) console.error(`FALHOU — ${f}`);
      process.exit(1);
    }
  });
