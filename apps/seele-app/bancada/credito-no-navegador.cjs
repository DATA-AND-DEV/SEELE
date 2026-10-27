// **As regras do crédito (0x04) da página, num navegador real** (Δ8 da linha 2).
//
//   node apps/seele-app/bancada/credito-no-navegador.cjs <chromium|webkit> <caminho do playwright> [--json saída] [--reverter inicio|agregado]
//
// O `rtc-web.js` de produto, carregado na página, com canais de mentira no
// lugar dos `RTCDataChannel` (o par é a bancada, que vê cada byte que sai e
// decide quando anunciar). O que se prova:
//
// 1. **o crédito inicial atrasado**: quem escreve 3 MiB não manda NENHUM dado
//    antes do primeiro anúncio (o limite começa em 0), espera sem cortar, e
//    depois sai exatamente até cada limite anunciado, byte a byte;
// 2. **o crédito zero**: um anúncio de 0 não solta nada; o fim de um fluxo
//    vazio e o corte saem sem crédito nenhum;
// 3. **muitos canais**: 20 canais que a página lê, 2 MiB cada, com um par que
//    respeita o crédito e leitores lentos em ordem aleatória. O concedido e não
//    lido nunca passa de `MAX_CREDITO_DA_CONEXAO` (8 MiB), nenhum canal é
//    cortado, e todos os bytes chegam em ordem;
// 4. **o fim sem leitor** (o defeito que o Codex mediu): 9 canais de 1 MiB, o par
//    manda até o crédito e o FIM, e ninguém lê. O que espera leitura de verdade
//    (o que entrou menos o que saiu, contado pela bancada) nunca passa de
//    8 MiB: o FIM não tira da soma o que ainda está na fila. Depois os leitores
//    drenam, o nono canal recebe o crédito que sobrou, e no fim a soma volta a
//    0 — a cota é devolvida, sem vazamento.
//
// `--reverter inicio` serve o `rtc-web.js` com o limite começando sem teto (o
// «antes do primeiro anúncio, não espera»); `--reverter agregado`, sem a soma
// por conexão; `--reverter fim`, com o FIM tirando o canal da soma na hora (o
// defeito); `--reverter vazamento`, com o fluxo fechado no FIM (o que se lê
// depois não volta como cota).
// Só na memória; a prova do caso dela tem de reprovar.

"use strict";

const fs = require("fs");
const http = require("http");
const path = require("path");

const argumentos = process.argv.slice(2);
const opcao = (nome) => (argumentos.includes(nome) ? argumentos[argumentos.indexOf(nome) + 1] : null);
const [navegador = "chromium", playwright] = argumentos.filter((a, i) => !a.startsWith("--") && !["--json", "--reverter"].includes(argumentos[i - 1]));
const saidaJson = opcao("--json");
const REVERTER = opcao("--reverter");
if (!playwright) {
  console.error("uso: node credito-no-navegador.cjs <chromium|webkit> <playwright> [--json saída] [--reverter inicio|agregado]");
  process.exit(2);
}
const UI = path.join(__dirname, "..", "ui");

function fonte() {
  let texto = fs.readFileSync(path.join(UI, "rtc-web.js"), "utf8");
  const reversoes = {
    inicio: ["function criarCreditoDoPar(canal, contas = null) {\n  let limite = 0;", "function criarCreditoDoPar(canal, contas = null) {\n  let limite = Infinity;"],
    agregado: ["if (!um.fixo) novo = Math.min(desejado, um.limite + Math.max(0, total - emAberto()));", "if (!um.fixo) novo = desejado;"],
    fim: ["          if (drenou()) sair(um);\n          else reverTodos();", "          sair(um);"],
    // O fluxo fechado no FIM, com dados ainda na fila: o que o leitor tira
    // depois não é mais contado como consumido, e a cota nunca volta.
    vazamento: ["      janela?.fimDoPar();\n      fecharSeEsvaziou();", "      janela?.fimDoPar();\n      fimPendente = false;\n      controlador.close();"],
  };
  if (REVERTER) {
    const [de, para] = reversoes[REVERTER] ?? [];
    if (!de || !texto.includes(de)) throw new Error(`a reversão «${REVERTER}» não casa com o rtc-web.js`);
    texto = texto.replace(de, para);
    console.error(`credito-no-navegador: REVERSÃO «${REVERTER}» no rtc-web.js servido`);
  }
  return texto;
}

function servir() {
  const js = fonte();
  const servidor = http.createServer((pedido, resposta) => {
    if (pedido.url === "/rtc-web.js") {
      resposta.writeHead(200, { "Content-Type": "text/javascript" });
      resposta.end(js);
      return;
    }
    resposta.writeHead(200, { "Content-Type": "text/html" });
    resposta.end('<!doctype html><meta charset="utf-8"><script src="/rtc-web.js"></script>');
  });
  return new Promise((ok) => servidor.listen(0, "127.0.0.1", () => ok(servidor)));
}

/** Na página. */
async function provar() {
  const R = {};
  const espera = (ms) => new Promise((r) => setTimeout(r, ms));
  const MiB = 1024 * 1024;

  /** Um canal de mentira: o que a página manda fica em `saidas`. */
  function canalDeMentira() {
    const canal = new EventTarget();
    canal.readyState = "open";
    canal.bufferedAmount = 0;
    canal.saidas = [];
    canal.send = (bytes) => {
      canal.saidas.push(new Uint8Array(bytes instanceof ArrayBuffer ? bytes : bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength)));
    };
    canal.close = () => {
      canal.readyState = "closed";
      canal.dispatchEvent(new Event("close"));
    };
    canal.chegar = (bytes) => canal.dispatchEvent(new MessageEvent("message", { data: bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) }));
    return canal;
  }
  const dadosQueSairam = (canal) =>
    canal.saidas.filter((m) => m[0] === ENVELOPE.dados).reduce((t, m) => t + m.length - 1, 0);
  const padrao = (inicio, n) => {
    const p = new Uint8Array(n);
    for (let i = 0; i < n; i++) p[i] = (inicio + i) % 253;
    return p;
  };

  // ------------------------------------ 1. o crédito inicial atrasado
  {
    const canal = canalDeMentira();
    const credito = criarCreditoDoPar(canal);
    const escrita = escritaDoCanal(canal, { fecharNoFim: false, credito }).getWriter();
    const TOTAL = 3 * MiB;
    const escrevendo = escrita.write(padrao(0, TOTAL)).then(() => "escreveu", (falha) => ({ falha: String(falha) }));
    await espera(500);
    const antesDoAnuncio = dadosQueSairam(canal);
    credito.subir(MiB);
    await espera(100);
    const depoisDeUmMiB = dadosQueSairam(canal);
    credito.subir(3 * MiB);
    const resultado = await escrevendo;
    const junto = [];
    for (const m of canal.saidas) if (m[0] === ENVELOPE.dados) junto.push(...m.subarray(1));
    R.inicialAtrasado = {
      antesDoAnuncio,
      depoisDeUmMiB,
      resultado,
      total: dadosQueSairam(canal),
      iguais: junto.length === TOTAL && junto.every((b, i) => b === i % 253),
    };
  }

  // ------------------------------------------- 2. o crédito zero
  {
    const canal = canalDeMentira();
    const credito = criarCreditoDoPar(canal);
    const escrita = escritaDoCanal(canal, { fecharNoFim: false, credito }).getWriter();
    credito.subir(0);
    const escrevendo = escrita.write(padrao(0, 64 * 1024)).then(() => "escreveu", (falha) => ({ falha: String(falha) }));
    await espera(300);
    const comZero = dadosQueSairam(canal);
    credito.subir(64 * 1024);
    await escrevendo;
    // O fim de um fluxo vazio, sem crédito nenhum.
    const vazio = canalDeMentira();
    const escritaVazia = escritaDoCanal(vazio, { fecharNoFim: false, credito: criarCreditoDoPar(vazio) }).getWriter();
    const fechou = await Promise.race([escritaVazia.close().then(() => true), espera(500).then(() => false)]);
    // E o corte, sem crédito nenhum.
    const cortado = canalDeMentira();
    const escritaCortada = escritaDoCanal(cortado, { fecharNoFim: false, credito: criarCreditoDoPar(cortado) }).getWriter();
    await Promise.race([escritaCortada.abort(7), espera(500)]);
    R.creditoZero = {
      comZero,
      depois: dadosQueSairam(canal),
      fimSemCredito: fechou && vazio.saidas.some((m) => m[0] === ENVELOPE.fim),
      corteSemCredito: cortado.saidas.some((m) => m[0] === ENVELOPE.corte),
    };
  }

  // -------------------------------------------- 3. muitos canais
  {
    const conexao = criarCreditoDaConexao();
    const CANAIS = 20;
    const POR_CANAL = 2 * MiB;
    const PEDACO = MAX_DADOS;
    let maiorEmAberto = 0;
    let cortes = 0;
    const canais = Array.from({ length: CANAIS }, (_, indice) => {
      const canal = canalDeMentira();
      const janela = conexao.abrir(canal);
      const lado = leituraDoCanal(canal, { janela, aoCortarPorFila: () => (cortes += 1) });
      canal.addEventListener("message", ({ data }) => lado.chegou(data));
      // O par: escreve até o limite que a página anunciou, nunca além.
      const par = { limite: 0, enviados: 0 };
      const mandarAteOLimite = () => {
        while (par.enviados < POR_CANAL && par.enviados + Math.min(PEDACO, POR_CANAL - par.enviados) <= par.limite) {
          const n = Math.min(PEDACO, POR_CANAL - par.enviados);
          canal.chegar(envelopar(ENVELOPE.dados, padrao(par.enviados, n)));
          par.enviados += n;
        }
        if (par.enviados === POR_CANAL && !par.fim) {
          par.fim = true;
          canal.chegar(envelopar(ENVELOPE.fim));
        }
      };
      const enviarOriginal = canal.send;
      canal.send = (bytes) => {
        enviarOriginal(bytes);
        const lido = lerEnvelope(bytes);
        if (lido && lido.tag === ENVELOPE.credito) {
          par.limite = Math.max(par.limite, lido.limite);
          queueMicrotask(mandarAteOLimite);
        }
      };
      janela.anunciar();
      return { indice, canal, lado, par };
    });
    const vigia = setInterval(() => {
      maiorEmAberto = Math.max(maiorEmAberto, conexao.emAberto());
    }, 2);
    // Leitores lentos: cada um começa numa hora sorteada e lê um pedaço a cada volta.
    const lidos = await Promise.all(
      canais.map(async ({ indice, lado }) => {
        await espera(50 + ((indice * 37) % 400));
        const leitor = lado.leitura.getReader();
        let total = 0;
        let emOrdem = true;
        for (;;) {
          let lido;
          try {
            lido = await leitor.read();
          } catch (falha) {
            return { total, emOrdem, erro: String(falha?.corte !== undefined ? `corte ${falha.corte}` : falha) };
          }
          if (lido.done) break;
          for (let i = 0; i < lido.value.length; i++) {
            if (lido.value[i] !== (total + i) % 253) {
              emOrdem = false;
              break;
            }
          }
          total += lido.value.length;
          if (total % (256 * 1024) < PEDACO) await espera(5);
        }
        return { total, emOrdem, erro: null };
      }),
    );
    clearInterval(vigia);
    maiorEmAberto = Math.max(maiorEmAberto, conexao.emAberto());
    R.muitosCanais = {
      canais: CANAIS,
      porCanal: POR_CANAL,
      maiorEmAberto,
      teto: MAX_CREDITO_DA_CONEXAO,
      cortes,
      completos: lidos.filter((l) => l.total === POR_CANAL && l.emOrdem && l.erro === null).length,
      erros: lidos.filter((l) => l.erro).map((l) => l.erro),
      inicialZero: canais.filter(({ canal }) => {
        const primeiro = canal.saidas.find((m) => m[0] === ENVELOPE.credito);
        return primeiro && lerEnvelope(primeiro).limite === 0;
      }).length,
    };
  }
  // ---------------------------------------------- 4. o fim sem leitor
  {
    const conexao = criarCreditoDaConexao();
    const CANAIS = 9;
    const POR_CANAL = MiB;
    let retidoDeVerdade = 0;
    let maiorRetido = 0;
    const canais = Array.from({ length: CANAIS }, () => {
      const canal = canalDeMentira();
      const janela = conexao.abrir(canal);
      const lado = leituraDoCanal(canal, { janela });
      canal.addEventListener("message", ({ data }) => lado.chegou(data));
      const par = { limite: 0, enviados: 0, fim: false };
      const mandarAteOLimite = () => {
        while (par.enviados < POR_CANAL && par.enviados + Math.min(MAX_DADOS, POR_CANAL - par.enviados) <= par.limite) {
          const n = Math.min(MAX_DADOS, POR_CANAL - par.enviados);
          canal.chegar(envelopar(ENVELOPE.dados, padrao(par.enviados, n)));
          par.enviados += n;
          retidoDeVerdade += n;
          maiorRetido = Math.max(maiorRetido, retidoDeVerdade);
        }
        if (par.enviados === POR_CANAL && !par.fim) {
          par.fim = true;
          canal.chegar(envelopar(ENVELOPE.fim));
        }
      };
      const enviarOriginal = canal.send;
      canal.send = (bytes) => {
        enviarOriginal(bytes);
        const lido = lerEnvelope(bytes);
        if (lido && lido.tag === ENVELOPE.credito) {
          par.limite = Math.max(par.limite, lido.limite);
          // Numa tarefa, e não numa microtarefa: o leitor que acabou de tirar um
          // pedaço o conta antes de o par mandar o próximo (senão a conta da
          // bancada ficaria um pedaço atrás do que a página já entregou).
          setTimeout(mandarAteOLimite, 0);
        }
      };
      janela.anunciar();
      return { canal, lado, par };
    });
    await espera(100);
    const antesDeLer = { retido: retidoDeVerdade, emAberto: conexao.emAberto(), comFim: canais.filter((c) => c.par.fim).length };
    // Agora os leitores drenam, um canal de cada vez.
    let total = 0;
    let emOrdem = true;
    for (const { lado } of canais) {
      const leitor = lado.leitura.getReader();
      let deste = 0;
      for (;;) {
        const { value, done } = await Promise.race([leitor.read(), espera(3000).then(() => ({ done: true, prazo: true }))]);
        if (done) break;
        for (let i = 0; i < value.length; i++) {
          if (value[i] !== (deste + i) % 253) {
            emOrdem = false;
            break;
          }
        }
        deste += value.length;
        retidoDeVerdade -= value.length;
      }
      total += deste;
    }
    await espera(50);
    R.fimSemLeitor = {
      canais: CANAIS,
      porCanal: POR_CANAL,
      antesDeLer,
      maiorRetido,
      teto: MAX_CREDITO_DA_CONEXAO,
      total,
      emOrdem,
      emAbertoDepoisDeDrenar: conexao.emAberto(),
    };
  }
  return R;
}

(async () => {
  const servidor = await servir();
  const { chromium, webkit } = require(playwright);
  const browser = await (navegador === "webkit" ? webkit : chromium).launch();
  let R;
  try {
    const pagina = await browser.newPage();
    pagina.on("pageerror", (e) => console.error(`[página] ${e.message}`));
    await pagina.goto(`http://127.0.0.1:${servidor.address().port}/`);
    R = await pagina.evaluate(provar);
    R.navegador = `${navegador} ${browser.version()}`;
    R.reversao = REVERTER;
  } finally {
    await browser.close();
    servidor.close();
  }
  const MiB = 1024 * 1024;
  const a = R.inicialAtrasado;
  const z = R.creditoZero;
  const m = R.muitosCanais;
  R.veredito = {
    inicialAtrasado: a.antesDoAnuncio === 0 && a.depoisDeUmMiB > 0 && a.depoisDeUmMiB <= MiB && a.resultado === "escreveu" && a.total === 3 * MiB && a.iguais,
    creditoZero: z.comZero === 0 && z.depois === 64 * 1024 && z.fimSemCredito && z.corteSemCredito,
    muitosCanais: m.maiorEmAberto <= m.teto && m.cortes === 0 && m.completos === m.canais && m.erros.length === 0,
    fimSemLeitor:
      R.fimSemLeitor.maiorRetido <= R.fimSemLeitor.teto &&
      R.fimSemLeitor.total === R.fimSemLeitor.canais * R.fimSemLeitor.porCanal &&
      R.fimSemLeitor.emOrdem &&
      R.fimSemLeitor.emAbertoDepoisDeDrenar === 0,
  };
  const texto = JSON.stringify(R, null, 2);
  console.log(texto);
  if (saidaJson) fs.writeFileSync(saidaJson, texto + "\n");
  if (REVERTER) {
    const caso = { inicio: "inicialAtrasado", agregado: "muitosCanais", fim: "fimSemLeitor", vazamento: "fimSemLeitor" }[REVERTER];
    const reprovou = !R.veredito[caso];
    console.error(`credito-no-navegador: reversão «${REVERTER}» — ${caso} ${reprovou ? "REPROVOU" : "passou (a reversão não pegou)"}`);
    process.exit(reprovou ? 0 : 1);
  }
  process.exit(Object.values(R.veredito).every(Boolean) ? 0 : 1);
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
