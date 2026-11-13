// O passo de recorte da foto e da capa do perfil (1.0.1, pedido do dono: «às
// vezes a imagem é muito maior, um wallpaper, e eu quero uma parte
// específica»), num navegador de verdade, nas três cascas.
//
//     node apps/seele-app/bancada/recorte-do-perfil.cjs \
//       target/wasm32-unknown-unknown/release/seele_web.wasm
//
// (`cargo xtask web` compila o `.wasm`; `SEELE_PREVIEW_DIR` grava as telas.)
//
// # A imagem de teste
//
// Gerada na hora, num canvas da página: um gradiente em que **o vermelho diz a
// coluna e o verde diz a linha** de cada pixel, com um quadrado de cor cheia em
// cada canto (magenta em cima à esquerda, ciano em cima à direita, amarelo
// embaixo à esquerda, branco embaixo à direita). De qualquer pixel do que sai
// se lê de que lugar da imagem inteira ele veio.
//
// # O que ela mede
//
// **Na página, com a ponte e o `.wasm` de verdade** (a casca larga a 1600 × 919
// e a 900 × 600, e o celular a 360 × 640, com toque): o «Seu perfil» da tela
// inicial, ESCOLHER, o arquivo, o recorte, CONFIRMAR, e a imagem que o perfil
// padrão guardou — o caminho inteiro, sem dublê. Pelos pixels do que ficou
// guardado:
//
// 1. quem confirma direto recebe o pedaço centralizado que cobre a moldura;
// 2. arrastada e aproximada (o controle, a roda, a pinça de dois dedos), a
//    imagem sai no pedaço **que estava dentro da moldura** — o canto marcado
//    chega com a cor dele —, na proporção do lado (a foto quadrada, a capa
//    16:9) e dentro do teto de sempre (512; 1280 × 720);
// 3. um JPEG sai JPEG, e um GIF animado sai animado, recortado em cada quadro;
// 4. um papel de parede em 8K abre e sai, e a prévia que foi à página não
//    passa de 2048 px;
// 5. CANCELAR, Esc e apertar fora não mudam nada, e o «Seu perfil» continua
//    aberto, sem perguntar se descarta;
// 6. pelo teclado: as setas movem a moldura, `+` e `−` mudam o zoom, Enter
//    confirma, e o Tab não sai do diálogo.
//
// **No aplicativo, com a ponte do Tauri simulada** (a 1600 × 919, a 900 × 600 e
// a 360 × 640): dentro de um servidor, o pedaço que `enviar_imagem_do_perfil`
// recebe é o que estava dentro da moldura, em pixels inteiros da imagem
// inteira e na proporção exata; a imagem menor que a moldura, a muito
// alongada e a enorme abrem cobrindo a moldura, e nenhum gesto a tira de
// dentro da imagem; cancelar solta a imagem que esperava.
//
// **Nos três tamanhos, nas três cascas**: sem rolagem horizontal, o diálogo e
// cada controle dentro da janela, a moldura dentro do palco e na proporção, e
// no celular o diálogo ocupa a tela.

const assert = require("node:assert/strict");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");
const { servir, respostas, retrato } = require("./telas.cjs");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");
const engines = require("./playwright.cjs");

const [caminhoDoWasm] = process.argv.slice(2).filter((argumento) => !argumento.startsWith("--"));
if (!caminhoDoWasm || !fs.existsSync(caminhoDoWasm)) {
  console.error(
    "uso: node apps/seele-app/bancada/recorte-do-perfil.cjs <seele_web.wasm>\n" +
      "  (cargo xtask web, que o compila em target/wasm32-unknown-unknown/release/)",
  );
  process.exit(2);
}

const UI = path.resolve(__dirname, "../ui");
const PREVIAS = process.env.SEELE_PREVIEW_DIR || null;
if (PREVIAS) fs.mkdirSync(PREVIAS, { recursive: true });

/** Os três tamanhos de janela do pedido. */
const TAMANHOS = Object.freeze({ largo: [1600, 919], medio: [900, 600], celular: [360, 640] });

const TIPOS = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".json": "application/json",
  ".webmanifest": "application/manifest+json",
  ".woff2": "font/woff2",
};

/** A `ui/` com o `.wasm` ao lado, servida com os cabeçalhos da origem do web app. */
function servirAPagina() {
  const pasta = fs.mkdtempSync(path.join(os.tmpdir(), "recorte-do-perfil-"));
  fs.cpSync(UI, pasta, { recursive: true });
  fs.copyFileSync(caminhoDoWasm, path.join(pasta, "seele_web.wasm"));
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
  servidor.on("close", () => fs.rmSync(pasta, { recursive: true, force: true }));
  return servidor;
}

// ------------------------------------------------------ o que roda na página

/** As funções da bancada dentro da página: a imagem de teste, e a leitura do que saiu. */
function naPagina() {
  const MARCAS = { cimaEsquerda: [255, 0, 255], cimaDireita: [0, 255, 255], baixoEsquerda: [255, 255, 0], baixoDireita: [255, 255, 255] };
  /** O lado de cada quadrado de canto: 8% do menor lado da imagem. */
  const ladoDaMarca = (largura, altura) => Math.max(2, Math.round(Math.min(largura, altura) * 0.08));

  /** O gradiente com as marcas, num canvas. O vermelho é a coluna, o verde é a linha. */
  function desenharAImagemDeTeste(largura, altura) {
    const tela = document.createElement("canvas");
    tela.width = largura;
    tela.height = altura;
    const pincel = tela.getContext("2d");
    // Uma linha do gradiente de cada vez: uma imagem em 8K inteira num
    // `ImageData` são 133 MB de uma vez.
    const linha = pincel.createImageData(largura, 1);
    for (let x = 0; x < largura; x += 1) {
      linha.data[x * 4] = Math.round((x / Math.max(1, largura - 1)) * 255);
      linha.data[x * 4 + 2] = 128;
      linha.data[x * 4 + 3] = 255;
    }
    for (let y = 0; y < altura; y += 1) {
      const verde = Math.round((y / Math.max(1, altura - 1)) * 255);
      for (let x = 0; x < largura; x += 1) linha.data[x * 4 + 1] = verde;
      pincel.putImageData(linha, 0, y);
    }
    const lado = ladoDaMarca(largura, altura);
    const quadrado = (cor, x, y) => {
      pincel.fillStyle = `rgb(${cor.join(",")})`;
      pincel.fillRect(x, y, lado, lado);
    };
    quadrado(MARCAS.cimaEsquerda, 0, 0);
    quadrado(MARCAS.cimaDireita, largura - lado, 0);
    quadrado(MARCAS.baixoEsquerda, 0, altura - lado);
    quadrado(MARCAS.baixoDireita, largura - lado, altura - lado);
    return tela;
  }

  const emBase64 = async (blob) => {
    const bytes = new Uint8Array(await blob.arrayBuffer());
    let binario = "";
    for (let i = 0; i < bytes.length; i += 0x8000) binario += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    return btoa(binario);
  };

  /** A imagem de teste como arquivo (base64), no `tipo` pedido. */
  async function imagemDeTeste(largura, altura, tipo = "image/png") {
    const tela = desenharAImagemDeTeste(largura, altura);
    const blob = await new Promise((ok) => tela.toBlob(ok, tipo, 0.95));
    return { base64: await emBase64(blob), bytes: blob.size };
  }

  /** A imagem de teste como `data:`, para o dublê do aplicativo. */
  function imagemDeTesteEmUri(largura, altura) {
    return desenharAImagemDeTeste(largura, altura).toDataURL("image/png");
  }

  /**
   * **Um GIF animado de dois quadros**, escrito à mão (sem codificador): a
   * metade esquerda de cada quadro de uma cor e a direita de outra, e as cores
   * trocam do primeiro para o segundo quadro. O LZW vai sem comprimir — um
   * código de limpar a cada dois pixels mantém o código em três bits.
   */
  function gifAnimado(largura, altura) {
    const bytes = [];
    const palavra = (n) => bytes.push(n & 0xff, (n >> 8) & 0xff);
    const texto = (t) => { for (const c of t) bytes.push(c.charCodeAt(0)); };
    texto("GIF89a");
    palavra(largura);
    palavra(altura);
    bytes.push(0xf1, 0, 0); // tabela global de 4 cores
    bytes.push(220, 30, 30, 30, 60, 220, 30, 200, 60, 240, 240, 240);
    bytes.push(0x21, 0xff, 11);
    texto("NETSCAPE2.0");
    bytes.push(3, 1, 0, 0, 0); // repete para sempre
    for (const quadro of [0, 1]) {
      bytes.push(0x21, 0xf9, 4, 0, 20, 0, 0, 0); // 200 ms
      bytes.push(0x2c);
      palavra(0);
      palavra(0);
      palavra(largura);
      palavra(altura);
      bytes.push(0, 2); // sem tabela local; código mínimo de 2 bits
      const bits = [];
      const codigo = (c) => { for (let b = 0; b < 3; b += 1) bits.push((c >> b) & 1); };
      let desdeOLimpar = 2;
      for (let y = 0; y < altura; y += 1) {
        for (let x = 0; x < largura; x += 1) {
          if (desdeOLimpar === 2) {
            codigo(4);
            desdeOLimpar = 0;
          }
          const esquerda = x < largura / 2;
          codigo(quadro === 0 ? (esquerda ? 0 : 1) : esquerda ? 2 : 3);
          desdeOLimpar += 1;
        }
      }
      codigo(5);
      const dados = [];
      for (let i = 0; i < bits.length; i += 8) {
        let byte = 0;
        for (let b = 0; b < 8 && i + b < bits.length; b += 1) byte |= bits[i + b] << b;
        dados.push(byte);
      }
      for (let i = 0; i < dados.length; i += 255) {
        const bloco = dados.slice(i, i + 255);
        bytes.push(bloco.length, ...bloco);
      }
      bytes.push(0);
    }
    bytes.push(0x3b);
    return emBase64(new Blob([new Uint8Array(bytes)]));
  }

  /** Quantos quadros um GIF tem, andando pelos blocos dele. */
  function quadrosDoGif(bytes) {
    let i = 13 + (bytes[10] & 0x80 ? 3 * 2 ** ((bytes[10] & 7) + 1) : 0);
    let quadros = 0;
    const pularSubBlocos = () => {
      while (bytes[i] !== 0) i += bytes[i] + 1;
      i += 1;
    };
    while (i < bytes.length && bytes[i] !== 0x3b) {
      if (bytes[i] === 0x21) {
        i += 2;
        pularSubBlocos();
      } else if (bytes[i] === 0x2c) {
        quadros += 1;
        const local = bytes[i + 9];
        i += 10 + (local & 0x80 ? 3 * 2 ** ((local & 7) + 1) : 0) + 1;
        pularSubBlocos();
      } else {
        return -1;
      }
    }
    return quadros;
  }

  /**
   * **O que saiu**, lido dos bytes guardados: as medidas, o tipo, e de onde
   * vem cada borda — o vermelho do meio da borda esquerda diz a coluna em que
   * o recorte começa (de 0 a 1 da imagem inteira), e assim as outras três. E
   * a cor de cada canto, um pouco para dentro.
   */
  async function lerOQueSaiu(bytes) {
    const tipo = bytes[0] === 0x89 ? "image/png" : bytes[0] === 0xff ? "image/jpeg" : bytes[0] === 0x47 ? "image/gif" : "image/webp";
    // De uma animação, o primeiro quadro.
    const imagem = await createImageBitmap(new Blob([bytes], { type: tipo }));
    const tela = document.createElement("canvas");
    tela.width = imagem.width;
    tela.height = imagem.height;
    const pincel = tela.getContext("2d", { willReadFrequently: true });
    pincel.drawImage(imagem, 0, 0);
    const l = tela.width;
    const a = tela.height;
    const em = (fx, fy) => {
      const x = Math.min(l - 1, Math.max(0, Math.round(fx * (l - 1))));
      const y = Math.min(a - 1, Math.max(0, Math.round(fy * (a - 1))));
      return [...pincel.getImageData(x, y, 1, 1).data].slice(0, 3);
    };
    // O ponto de uma borda que não caiu num quadrado de canto: no gradiente
    // o azul é sempre 128, e nas quatro marcas é 0 ou 255.
    const naBorda = (ponto) => {
      for (const ao_longo of [0.5, 0.1, 0.9, 0.3, 0.7]) {
        const cor = ponto(ao_longo);
        if (Math.abs(cor[2] - 128) <= 10) return cor;
      }
      return [NaN, NaN, NaN];
    };
    return {
      tipo,
      bytes: bytes.length,
      quadros: tipo === "image/gif" ? quadrosDoGif(bytes) : 1,
      largura: l,
      altura: a,
      regiao: {
        x0: naBorda((t) => em(0, t))[0] / 255,
        x1: naBorda((t) => em(1, t))[0] / 255,
        y0: naBorda((t) => em(t, 0))[1] / 255,
        y1: naBorda((t) => em(t, 1))[1] / 255,
      },
      cantos: { cimaEsquerda: em(0.02, 0.02), cimaDireita: em(0.98, 0.02), baixoEsquerda: em(0.02, 0.98), baixoDireita: em(0.98, 0.98) },
      meio: em(0.5, 0.5),
    };
  }

  /** O diálogo como ele está na tela: os retângulos, e o pedaço que a moldura pega (de 0 a 1 da imagem). */
  function geometria() {
    const ret = (id) => {
      const r = document.getElementById(id).getBoundingClientRect();
      return { x: r.left, y: r.top, largura: r.width, altura: r.height, direita: r.right, baixo: r.bottom };
    };
    const moldura = ret("recorte-moldura");
    const imagem = ret("recorte-imagem");
    const caixa = document.querySelector("#recorte .recorte-caixa").getBoundingClientRect();
    const ids = ["recorte-titulo", "recorte-ajuda", "recorte-palco", "recorte-moldura", "recorte-menos", "recorte-zoom", "recorte-mais",
      "recorte-zoom-valor", "recorte-cancelar", "recorte-confirmar"];
    const cortados = ["recorte-titulo", "recorte-cancelar", "recorte-confirmar", "recorte-zoom-rotulo", "recorte-zoom-valor"]
      .filter((id) => { const e = document.getElementById(id); return e.scrollWidth > e.clientWidth + 1; });
    return {
      janela: { largura: innerWidth, altura: innerHeight },
      rolagem: { pagina: document.documentElement.scrollWidth, corpo: document.body.scrollWidth },
      caixa: { x: caixa.left, y: caixa.top, largura: caixa.width, altura: caixa.height, direita: caixa.right, baixo: caixa.bottom },
      palco: ret("recorte-palco"),
      moldura,
      imagem,
      controles: Object.fromEntries(ids.map((id) => [id, ret(id)])),
      cortados,
      nota: document.getElementById("recorte-nota").hidden ? "" : document.getElementById("recorte-nota").textContent,
      zoom: Number(document.getElementById("recorte-zoom").value),
      zoomMaximo: Number(document.getElementById("recorte-zoom").max),
      zoomParado: document.getElementById("recorte-zoom").disabled,
      vista: {
        x0: (moldura.x - imagem.x) / imagem.largura,
        y0: (moldura.y - imagem.y) / imagem.altura,
        x1: (moldura.direita - imagem.x) / imagem.largura,
        y1: (moldura.baixo - imagem.y) / imagem.altura,
      },
      lado: document.getElementById("recorte").dataset.lado,
      foco: document.activeElement?.id ?? null,
    };
  }

  window.bancadaDoRecorte = { MARCAS, ladoDaMarca, imagemDeTeste, imagemDeTesteEmUri, gifAnimado, lerOQueSaiu, geometria };
}

// --------------------------------------------------------- o que confere

const perto = (a, b, folga) => Math.abs(a - b) <= folga;
const mesmaCor = (cor, esperada) => cor.every((canal, i) => Math.abs(canal - esperada[i]) <= 24);
const MARCAS = { cimaEsquerda: [255, 0, 255], cimaDireita: [0, 255, 255], baixoEsquerda: [255, 255, 0], baixoDireita: [255, 255, 255] };
const PROPORCAO = { Foto: 1, Capa: 16 / 9 };
const TETO = { Foto: [512, 512], Capa: [1280, 720] };

const geometria = (page) => page.evaluate(() => bancadaDoRecorte.geometria());

/**
 * **O diálogo cabe**: sem rolagem horizontal, a caixa e cada controle dentro
 * da janela, a moldura dentro do palco e na proporção do lado, nenhum texto
 * cortado; na janela estreita, a caixa é a tela. E a imagem cobre a moldura.
 */
async function conferirOLayout(page, onde) {
  const g = await geometria(page);
  const { largura, altura } = g.janela;
  assert.ok(g.rolagem.pagina <= largura && g.rolagem.corpo <= largura, `${onde}: a página rola de lado (${JSON.stringify(g.rolagem)} em ${largura})`);
  const dentro = (r) => r.largura > 0 && r.altura > 0 && r.x >= -0.5 && r.y >= -0.5 && r.direita <= largura + 0.5 && r.baixo <= altura + 0.5;
  assert.ok(dentro(g.caixa), `${onde}: a caixa do recorte sai da janela: ${JSON.stringify(g.caixa)}`);
  for (const [id, r] of Object.entries(g.controles)) {
    assert.ok(dentro(r), `${onde}: #${id} sai da janela ou não aparece: ${JSON.stringify(r)}`);
    assert.ok(r.x >= g.caixa.x - 0.5 && r.direita <= g.caixa.direita + 0.5 && r.y >= g.caixa.y - 0.5 && r.baixo <= g.caixa.baixo + 0.5,
      `${onde}: #${id} sai da caixa: ${JSON.stringify(r)}`);
  }
  assert.deepEqual(g.cortados, [], `${onde}: texto cortado`);
  assert.ok(g.moldura.x >= g.palco.x && g.moldura.direita <= g.palco.direita && g.moldura.y >= g.palco.y && g.moldura.baixo <= g.palco.baixo,
    `${onde}: a moldura sai do palco`);
  assert.ok(perto(g.moldura.largura / g.moldura.altura, PROPORCAO[g.lado], 0.03),
    `${onde}: a moldura da ${g.lado} mede ${g.moldura.largura} × ${g.moldura.altura}`);
  assert.ok(g.moldura.largura >= 120, `${onde}: a moldura ficou com ${g.moldura.largura}px de largura`);
  // As ações não ficam debaixo do palco nem uma sobre a outra.
  assert.ok(g.controles["recorte-confirmar"].y >= g.palco.baixo - 0.5, `${onde}: o CONFIRMAR está sobre o palco`);
  assert.ok(g.controles["recorte-cancelar"].direita <= g.controles["recorte-confirmar"].x + 0.5, `${onde}: CANCELAR e CONFIRMAR se sobrepõem`);
  if (largura <= 700) {
    assert.ok(perto(g.caixa.largura, largura, 1) && perto(g.caixa.altura, altura, 1), `${onde}: na janela estreita o diálogo não ocupa a tela: ${JSON.stringify(g.caixa)}`);
  }
  conferirQueCobre(g, onde);
  return g;
}

/** A moldura nunca sai de dentro da imagem: o zoom mínimo é o que a cobre. */
function conferirQueCobre(g, onde) {
  const v = g.vista;
  assert.ok(v.x0 >= -0.002 && v.y0 >= -0.002 && v.x1 <= 1.002 && v.y1 <= 1.002, `${onde}: a moldura saiu da imagem: ${JSON.stringify(v)}`);
}

/** O que saiu é o pedaço que estava na moldura, na proporção do lado e dentro do teto. */
function conferirOQueSaiu(saiu, vista, lado, onde, folga = 0.02) {
  for (const borda of ["x0", "x1", "y0", "y1"]) {
    assert.ok(perto(saiu.regiao[borda], vista[borda], folga),
      `${onde}: a borda ${borda} do que saiu vem de ${saiu.regiao[borda].toFixed(3)} da imagem, e a moldura estava em ${vista[borda].toFixed(3)}`);
  }
  assert.ok(perto(saiu.largura / saiu.altura, PROPORCAO[lado], 0.01), `${onde}: saiu ${saiu.largura} × ${saiu.altura}, fora da proporção da ${lado}`);
  assert.ok(saiu.largura <= TETO[lado][0] && saiu.altura <= TETO[lado][1], `${onde}: saiu ${saiu.largura} × ${saiu.altura}, acima do teto da ${lado}`);
}

/** O prazo de cada espera: folgado para o 8K num runner lento, curto para uma reversão não durar a manhã. */
const PRAZO = 20_000;

async function esperarORecorte(page, aberto = true) {
  await page.waitForFunction((aberto) => document.getElementById("recorte").hidden !== aberto, aberto, { timeout: PRAZO });
  if (aberto) {
    // A prévia decodificada e desenhada: o retrato tem de mostrar a imagem.
    await page.waitForFunction(() => { const i = document.getElementById("recorte-imagem"); return i.complete && i.naturalWidth > 0; });
    await page.evaluate(() => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok))));
  }
}

const retratar = async (page, nome) => {
  if (PREVIAS) await page.screenshot({ path: path.join(PREVIAS, `${nome}.png`) });
};

/** Arrasta a imagem `dx`, `dy` pixels com o mouse, a partir do meio do palco. */
async function arrastar(page, dx, dy) {
  const g = await geometria(page);
  const x = g.palco.x + g.palco.largura / 2;
  const y = g.palco.y + g.palco.altura / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  // Em passos, e para além do palco: a captura do ponteiro segura o arrasto.
  for (let passo = 1; passo <= 8; passo += 1) await page.mouse.move(x + (dx * passo) / 8, y + (dy * passo) / 8);
  await page.mouse.up();
}

/** Põe o controle de zoom num valor, como quem o arrasta. */
async function porOZoom(page, valor) {
  await page.evaluate((valor) => {
    const zoom = document.getElementById("recorte-zoom");
    zoom.value = String(valor);
    zoom.dispatchEvent(new Event("input", { bubbles: true }));
  }, valor);
}

// ------------------------------------------------- a página, com o `.wasm`

/** Abre o «Seu perfil» da tela inicial: o perfil padrão desta origem. */
async function abrirOPerfilPadrao(page) {
  await page.evaluate(() => abrirPerfil());
  await page.waitForFunction(() => !document.getElementById("perfil").hidden && document.getElementById("perfil").dataset.modo === "padrao");
}

/** ESCOLHER, e o arquivo dado ao seletor do navegador. */
async function escolher(page, lado, arquivo) {
  const [seletor] = await Promise.all([
    page.waitForEvent("filechooser"),
    page.click(lado === "Capa" ? "#perfil-capa-escolher" : "#perfil-foto-escolher"),
  ]);
  await seletor.setFiles({ name: arquivo.nome, mimeType: arquivo.tipo, buffer: Buffer.from(arquivo.base64, "base64") });
}

/**
 * **A imagem que o perfil padrão guarda de um lado, lida pelos pixels**; `null`
 * sem imagem. Os bytes são os do armazém desta origem (o IndexedDB
 * `seele-perfil-padrao`, pelo hash) — os que sobem a cada servidor —, e não o
 * `data:` que o `perfil_padrao` devolve: esse já passa pela redução para a
 * tela, e mostraria dentro do teto uma imagem guardada fora dele.
 */
async function guardada(page, lado) {
  return page.evaluate(async (lado) => {
    const padrao = await invoke("perfil_padrao");
    const imagem = lado === "Capa" ? padrao.capa : padrao.foto;
    if (!imagem) return null;
    const bytes = await new Promise((ok, falhou) => {
      const abertura = indexedDB.open("seele-perfil-padrao");
      abertura.onerror = () => falhou(abertura.error);
      abertura.onsuccess = () => {
        const pedido = abertura.result.transaction("imagens").objectStore("imagens").get(imagem.hash);
        pedido.onerror = () => falhou(pedido.error);
        pedido.onsuccess = () => {
          abertura.result.close();
          ok(pedido.result);
        };
      };
    });
    if (!(bytes instanceof Uint8Array)) throw new Error(`o armazém não tem a ${lado} de hash ${imagem.hash}`);
    if (bytes.length !== imagem.bytes) throw new Error(`o padrão diz ${imagem.bytes} bytes e o armazém guarda ${bytes.length}`);
    if (!String(imagem.url).startsWith("data:image/")) throw new Error(`o perfil padrão não deu a ${lado} para a tela`);
    return { hash: imagem.hash, declarado: imagem.tipo, ...(await bancadaDoRecorte.lerOQueSaiu(bytes)) };
  }, lado);
}

/** Espera o bloco do lado dizer que guardou (ou a recusa, que reprova com a frase). */
async function esperarGuardar(page, lado) {
  const id = lado === "Capa" ? "perfil-capa-estado" : "perfil-foto-estado";
  await page.waitForFunction((id) => ["feito", "erro"].includes(document.getElementById(id).dataset.tom), id, { timeout: PRAZO });
  const estado = await page.evaluate((id) => ({ tom: document.getElementById(id).dataset.tom, texto: document.getElementById(id).textContent }), id);
  assert.equal(estado.tom, "feito", `o bloco da ${lado} recusou: ${estado.texto}`);
}

async function naPaginaDeVerdade(browser, origem, casca, [largura, altura], falhas) {
  const onde = `${casca} ${largura}×${altura}`;
  const contexto = await browser.newContext({
    viewport: { width: largura, height: altura },
    hasTouch: casca === "celular",
    isMobile: casca === "celular",
  });
  const page = await contexto.newPage();
  page.setDefaultTimeout(PRAZO);
  const erros = [];
  page.on("pageerror", (e) => erros.push(e.message));
  await page.addInitScript(naPagina);
  await page.goto(origem);
  await page.waitForFunction(() => typeof abrirPerfil === "function" && typeof recortarAImagem === "function");
  assert.equal(await page.evaluate(() => document.documentElement.dataset.casca), casca, `${onde}: a casca desta carga`);
  await abrirOPerfilPadrao(page);

  const fazer = async (caso, f) => {
    try {
      await f();
    } catch (falha) {
      falhas.push(`${onde} · ${caso}: ${falha.message.split("\n")[0]}`);
      await retratar(page, `falha-${casca}-${largura}-${caso.replace(/[^a-z0-9]+/gi, "-")}`);
      // O caso seguinte começa do «Seu perfil» aberto, sem recorte por cima.
      await page.evaluate(async () => {
        if (!document.getElementById("recorte").hidden) document.getElementById("recorte-cancelar").click();
        if (document.getElementById("perfil").hidden) await abrirPerfil();
      }).catch(() => {});
    }
  };

  const papel = { nome: "papel.png", tipo: "image/png", ...(await page.evaluate(() => bancadaDoRecorte.imagemDeTeste(4000, 2400))) };

  // 1. Quem não quer recortar confirma direto: o centralizado que cobre a moldura.
  await fazer("confirma direto", async () => {
    assert.equal(await guardada(page, "Foto"), null, "o perfil padrão desta origem já tinha foto");
    await escolher(page, "Foto", papel);
    await esperarORecorte(page);
    const g = await conferirOLayout(page, `${onde} foto`);
    await retratar(page, `recorte-foto-${casca}-${largura}x${altura}`);
    assert.equal(g.foco, "recorte-palco", "o foco não nasceu na área da imagem");
    assert.equal(g.zoom, 1, "o zoom não abriu no mínimo");
    // 4000 × 2400 numa moldura quadrada: o meio, de 0,2 a 0,8 da largura.
    assert.ok(perto(g.vista.x0, 0.2, 0.005) && perto(g.vista.x1, 0.8, 0.005) && perto(g.vista.y0, 0, 0.005) && perto(g.vista.y1, 1, 0.005),
      `o recorte inicial não é o centralizado que cobre: ${JSON.stringify(g.vista)}`);
    await page.click("#recorte-confirmar");
    await esperarORecorte(page, false);
    await esperarGuardar(page, "Foto");
    const saiu = await guardada(page, "Foto");
    conferirOQueSaiu(saiu, g.vista, "Foto", "confirma direto");
    assert.deepEqual([saiu.largura, saiu.altura, saiu.tipo], [512, 512, "image/png"]);
    // A amostra do editor mostra a foto nova.
    assert.match(await page.evaluate(() => document.getElementById("perfil-foto-amostra").style.backgroundImage), /^url\("data:image\/png/);
  });

  // 2. Aproximada e arrastada até o canto de cima à esquerda: sai o canto.
  await fazer("arrasta e aproxima", async () => {
    await escolher(page, "Foto", papel);
    await esperarORecorte(page);
    await porOZoom(page, 3);
    // A roda também aproxima (e não rola a página atrás).
    const antes = (await geometria(page)).zoom;
    const g0 = await geometria(page);
    await page.mouse.move(g0.palco.x + g0.palco.largura / 2, g0.palco.y + g0.palco.altura / 2);
    if (casca !== "celular") {
      await page.mouse.wheel(0, -200);
      await page.waitForFunction((antes) => Number(document.getElementById("recorte-zoom").value) > antes, antes);
    }
    // Arrastar para baixo e para a direita traz o canto de cima à esquerda.
    await arrastar(page, 3000, 3000);
    const g = await geometria(page);
    conferirQueCobre(g, "arrastada");
    assert.ok(g.zoom >= 3 && perto(g.vista.x0, 0, 0.003) && perto(g.vista.y0, 0, 0.003), `o arrasto não levou a moldura ao canto: ${JSON.stringify(g.vista)}`);
    await retratar(page, `recorte-foto-canto-${casca}-${largura}x${altura}`);
    await page.click("#recorte-confirmar");
    await esperarORecorte(page, false);
    await esperarGuardar(page, "Foto");
    const saiu = await guardada(page, "Foto");
    conferirOQueSaiu(saiu, g.vista, "Foto", "arrasta e aproxima");
    assert.ok(mesmaCor(saiu.cantos.cimaEsquerda, MARCAS.cimaEsquerda), `o canto de cima à esquerda não trouxe a marca dele: ${saiu.cantos.cimaEsquerda}`);
    assert.ok(!mesmaCor(saiu.cantos.baixoDireita, MARCAS.baixoDireita), "o canto de baixo à direita não devia estar no recorte");
    assert.ok(saiu.largura === saiu.altura && saiu.largura <= 512, `saiu ${saiu.largura} × ${saiu.altura}`);
  });

  // 3. A capa, em 16:9: aproximada e levada ao canto de baixo à direita.
  await fazer("a capa", async () => {
    await escolher(page, "Capa", papel);
    await esperarORecorte(page);
    const aberta = await conferirOLayout(page, `${onde} capa`);
    assert.equal(aberta.lado, "Capa");
    assert.match(aberta.nota, /cartão do perfil/, "a nota da capa não diz o que as marcas são");
    await retratar(page, `recorte-capa-${casca}-${largura}x${altura}`);
    await porOZoom(page, 1.6);
    await arrastar(page, -3000, -3000);
    const g = await geometria(page);
    assert.ok(perto(g.vista.x1, 1, 0.003) && perto(g.vista.y1, 1, 0.003), `o arrasto não levou a moldura ao canto de baixo: ${JSON.stringify(g.vista)}`);
    await page.click("#recorte-confirmar");
    await esperarORecorte(page, false);
    await esperarGuardar(page, "Capa");
    const saiu = await guardada(page, "Capa");
    conferirOQueSaiu(saiu, g.vista, "Capa", "a capa");
    assert.ok(mesmaCor(saiu.cantos.baixoDireita, MARCAS.baixoDireita), `o canto de baixo à direita não trouxe a marca dele: ${saiu.cantos.baixoDireita}`);
    // 4000/1,6 = 2500 px de largura: acima do teto, sai em 1280 × 720.
    assert.deepEqual([saiu.largura, saiu.altura], [1280, 720]);
  });

  // 4. Cancelar não muda nada: CANCELAR, Esc e apertar fora, e o perfil fica aberto.
  await fazer("cancelar", async () => {
    const antes = await guardada(page, "Foto");
    // No celular, o voltar do aparelho também cancela — e fecha o recorte, não o perfil.
    for (const como of ["botao", "esc", "fora", ...(casca === "celular" ? ["voltar"] : [])]) {
      await escolher(page, "Foto", papel);
      await esperarORecorte(page);
      await porOZoom(page, 2);
      await arrastar(page, 400, 200);
      if (como === "botao") await page.click("#recorte-cancelar");
      else if (como === "esc") await page.keyboard.press("Escape");
      else if (como === "voltar") await page.evaluate(() => history.back());
      // Na janela estreita o diálogo é a tela, e não há fora para apertar.
      else if (largura <= 700) await page.keyboard.press("Escape");
      else await page.mouse.click(2, 2);
      await esperarORecorte(page, false);
      const depois = await page.evaluate(() => ({
        perfil: !document.getElementById("perfil").hidden,
        descartar: !document.getElementById("perfil-fim-descartar").hidden,
        estado: document.getElementById("perfil-foto-estado").textContent,
        desligado: document.getElementById("perfil-foto-escolher").disabled,
        foco: document.activeElement?.id,
      }));
      assert.deepEqual(depois, { perfil: true, descartar: false, estado: "", desligado: false, foco: "perfil-foto-escolher" }, `cancelar (${como})`);
      assert.equal((await guardada(page, "Foto")).hash, antes.hash, `cancelar (${como}) trocou a foto guardada`);
    }
  });

  // 5. Pelo teclado: as setas movem a moldura, + e − o zoom, o Tab fica, Enter confirma.
  await fazer("teclado", async () => {
    await escolher(page, "Foto", papel);
    await esperarORecorte(page);
    const inicio = await geometria(page);
    assert.equal(inicio.foco, "recorte-palco");
    await page.keyboard.press("+");
    await page.keyboard.press("+");
    await page.keyboard.press("-");
    const comZoom = await geometria(page);
    assert.ok(perto(comZoom.zoom, 1.15, 0.02), `+ + − deixou o zoom em ${comZoom.zoom}`);
    for (let i = 0; i < 3; i += 1) await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowDown");
    const movida = await geometria(page);
    assert.ok(movida.vista.x0 > comZoom.vista.x0 + 0.01, `a seta para a direita não levou a moldura para a direita: ${comZoom.vista.x0} → ${movida.vista.x0}`);
    assert.ok(movida.vista.y0 > comZoom.vista.y0 + 0.005, "a seta para baixo não levou a moldura para baixo");
    await page.keyboard.press("Shift+ArrowLeft");
    const voltou = await geometria(page);
    assert.ok(voltou.vista.x0 < movida.vista.x0 - 0.01, "Shift + seta para a esquerda não andou");
    conferirQueCobre(voltou, "teclado");
    // O Tab dá a volta inteira sem sair do diálogo, e o Shift+Tab também.
    const visitados = [];
    for (let i = 0; i < 7; i += 1) {
      await page.keyboard.press("Tab");
      visitados.push(await page.evaluate(() => (document.getElementById("recorte").contains(document.activeElement) ? document.activeElement.id : `FORA:${document.activeElement?.id}`)));
    }
    assert.deepEqual(visitados, ["recorte-menos", "recorte-zoom", "recorte-mais", "recorte-cancelar", "recorte-confirmar", "recorte-palco", "recorte-menos"]);
    await page.keyboard.press("Shift+Tab");
    assert.equal(await page.evaluate(() => document.activeElement.id), "recorte-palco");
    // No controle de zoom, a seta é dele; e o controle tem rótulo.
    const rotulo = await page.evaluate(() => {
      const zoom = document.getElementById("recorte-zoom");
      return document.getElementById(zoom.getAttribute("aria-labelledby"))?.textContent;
    });
    assert.equal(rotulo, "Zoom", "o controle de zoom não tem rótulo");
    const antesDoEnter = await geometria(page);
    await page.keyboard.press("Enter");
    await esperarORecorte(page, false);
    await esperarGuardar(page, "Foto");
    conferirOQueSaiu(await guardada(page, "Foto"), antesDoEnter.vista, "Foto", "teclado");
  });

  // 6. Um JPEG sai JPEG.
  await fazer("jpeg", async () => {
    const foto = { nome: "foto.jpg", tipo: "image/jpeg", ...(await page.evaluate(() => bancadaDoRecorte.imagemDeTeste(1600, 1200, "image/jpeg"))) };
    await escolher(page, "Capa", foto);
    await esperarORecorte(page);
    await porOZoom(page, 2);
    const g = await geometria(page);
    await page.click("#recorte-confirmar");
    await esperarORecorte(page, false);
    await esperarGuardar(page, "Capa");
    const saiu = await guardada(page, "Capa");
    assert.equal(saiu.tipo, "image/jpeg", "o recorte de um JPEG não saiu JPEG");
    conferirOQueSaiu(saiu, g.vista, "Capa", "jpeg", 0.03);
  });

  if (casca === "larga" && largura === 1600) {
    // 7. Um GIF animado continua animado, recortado em cada quadro.
    await fazer("gif animado", async () => {
      const gif = { nome: "animado.gif", tipo: "image/gif", base64: await page.evaluate(() => bancadaDoRecorte.gifAnimado(240, 120)) };
      await escolher(page, "Foto", gif);
      await esperarORecorte(page);
      const g = await geometria(page);
      assert.match(g.nota, /continua animada/, `a nota não diz o que acontece com a animação: «${g.nota}»`);
      // Para a direita: a metade direita de cada quadro.
      await arrastar(page, -2000, 0);
      await retratar(page, "recorte-gif-animado");
      await page.click("#recorte-confirmar");
      await esperarORecorte(page, false);
      await esperarGuardar(page, "Foto");
      const saiu = await guardada(page, "Foto");
      assert.equal(saiu.tipo, "image/gif", "o GIF animado não saiu GIF");
      assert.equal(saiu.quadros, 2, `o GIF animado saiu com ${saiu.quadros} quadro(s): o recorte o congelou`);
      assert.deepEqual([saiu.largura, saiu.altura], [120, 120]);
      // O primeiro quadro: só a metade direita (azul), e nada da esquerda (vermelha).
      assert.ok(mesmaCor(saiu.cantos.cimaEsquerda, [30, 60, 220]) && mesmaCor(saiu.cantos.baixoDireita, [30, 60, 220]),
        `o primeiro quadro não é a metade direita: ${JSON.stringify(saiu.cantos)}`);
    });

    // 8. Um papel de parede em 8K: abre, a prévia cabe no teto dela, e o canto sai.
    await fazer("8k", async () => {
      const enorme = { nome: "8k.png", tipo: "image/png", ...(await page.evaluate(() => bancadaDoRecorte.imagemDeTeste(7680, 4320))) };
      assert.ok(enorme.bytes < 10 * 1024 * 1024, `a imagem em 8K desta bancada tem ${enorme.bytes} bytes: passa do teto do arquivo`);
      const t0 = Date.now();
      await escolher(page, "Capa", enorme);
      await esperarORecorte(page);
      const abrir = Date.now() - t0;
      const previa = await page.evaluate(() => { const i = document.getElementById("recorte-imagem"); return [i.naturalWidth, i.naturalHeight]; });
      assert.deepEqual(previa, [2048, 1152], "a prévia de uma imagem em 8K foi à página fora do teto dela");
      await conferirOLayout(page, `${onde} 8k`);
      await porOZoom(page, 8);
      await arrastar(page, -9000, 9000);
      const g = await geometria(page);
      assert.equal(g.zoomMaximo, 8);
      assert.ok(perto(g.vista.x1, 1, 0.003) && perto(g.vista.y0, 0, 0.003), JSON.stringify(g.vista));
      await retratar(page, "recorte-capa-8k-zoom-8");
      const t1 = Date.now();
      await page.click("#recorte-confirmar");
      await esperarORecorte(page, false);
      await esperarGuardar(page, "Capa");
      const guardar = Date.now() - t1;
      const saiu = await guardada(page, "Capa");
      // 7680/8 = 960 px: um oitavo da imagem, tirado da imagem inteira e não da prévia (que daria 256 px).
      assert.deepEqual([saiu.largura, saiu.altura], [960, 540]);
      conferirOQueSaiu(saiu, g.vista, "Capa", "8k", 0.01);
      assert.ok(mesmaCor(saiu.cantos.cimaDireita, MARCAS.cimaDireita), `o canto de cima à direita não trouxe a marca dele: ${saiu.cantos.cimaDireita}`);
      console.log(`recorte-do-perfil: 8K (${enorme.bytes} bytes) na página: ${abrir} ms até o diálogo, ${guardar} ms do CONFIRMAR ao guardado`);
      assert.ok(abrir < 30_000 && guardar < 30_000, `uma imagem em 8K levou ${abrir} ms para abrir e ${guardar} ms para guardar`);
    });
  }

  if (casca === "celular") {
    // 9. No celular os gestos são toque: um dedo arrasta, dois dedos fazem a pinça.
    await fazer("toque", async () => {
      await escolher(page, "Foto", papel);
      await esperarORecorte(page);
      const g0 = await geometria(page);
      const cdp = await contexto.newCDPSession(page);
      const meio = { x: Math.round(g0.palco.x + g0.palco.largura / 2), y: Math.round(g0.palco.y + g0.palco.altura / 2) };
      const dedos = (afastamento, dx = 0) => [
        { x: meio.x - afastamento + dx, y: meio.y, id: 0 },
        { x: meio.x + afastamento + dx, y: meio.y, id: 1 },
      ];
      await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: dedos(30) });
      for (const afastamento of [45, 60, 75, 90]) await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: dedos(afastamento) });
      await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
      const comPinca = await geometria(page);
      assert.ok(comPinca.zoom > 2.5 && comPinca.zoom < 3.5, `afastar os dedos de 60 para 180 px deixou o zoom em ${comPinca.zoom}`);
      conferirQueCobre(comPinca, "pinça");
      // Um dedo arrasta: para a esquerda, a moldura vai para a direita da imagem.
      await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x: meio.x, y: meio.y, id: 0 }] });
      for (const dx of [-30, -60, -90]) await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [{ x: meio.x + dx, y: meio.y, id: 0 }] });
      await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
      const arrastada = await geometria(page);
      assert.ok(arrastada.vista.x0 > comPinca.vista.x0 + 0.02, `o dedo não arrastou a imagem: ${comPinca.vista.x0} → ${arrastada.vista.x0}`);
      conferirQueCobre(arrastada, "um dedo");
      assert.equal(await page.evaluate(() => scrollY), 0, "o gesto rolou a página atrás do diálogo");
      await retratar(page, `recorte-foto-toque-${casca}-${largura}x${altura}`);
      await page.tap("#recorte-confirmar");
      await esperarORecorte(page, false);
      await esperarGuardar(page, "Foto");
      conferirOQueSaiu(await guardada(page, "Foto"), arrastada.vista, "Foto", "toque");
    });
  }

  assert.deepEqual(erros, [], `${onde}: a página lançou`);
  await contexto.close();
}

// -------------------------------------------- o aplicativo, com a ponte simulada

async function noAplicativo(browser, origem, [largura, altura], falhas) {
  const onde = `aplicativo ${largura}×${altura}`;
  const page = await browser.newPage({ viewport: { width: largura, height: altura } });
  page.setDefaultTimeout(PRAZO);
  const erros = [];
  page.on("pageerror", (e) => erros.push(e.message));
  await page.addInitScript(naPagina);
  await page.addInitScript((tabela) => {
    window.testTable = tabela;
    window.chamadas = [];
    window.__TAURI__ = {
      core: {
        invoke: async (comando, args) => {
          chamadas.push({ comando, args });
          const resposta = testTable[comando];
          return typeof resposta === "function" ? resposta(args) : resposta ?? null;
        },
      },
      event: { listen: async () => () => {} },
      window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
    };
  }, respostas({ snapshot: null, microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
  await page.goto(origem);
  await page.waitForFunction(() => typeof abrirPerfil === "function" && typeof recortarAImagem === "function");
  // Dentro de um servidor com perfis oficiais: o «Seu perfil» manda a imagem ao servidor.
  await page.evaluate(async (r) => {
    r.perfis_revision = 1;
    testTable.snapshot = r;
    testTable.perfis = [{ pessoa: r.me, revisao: 1, nome_exibido: "Alex", pronomes: "", status: "", bio: "", cor: "#f2521f", efeito: "Nenhum", foto: null, capa: null }];
    testTable.enviar_imagem_do_perfil = () => ({ icone_nativo: true });
    document.getElementById("tela-boot").hidden = true;
    document.getElementById("tela-sessao").hidden = false;
    desenhar(r);
    await new Promise((ok) => setTimeout(ok, 120));
    desenhar(r);
    await abrirPerfil();
  }, retrato());
  await page.waitForFunction(() => document.getElementById("perfil").dataset.modo === "oficial");

  /** O que `escolher_imagem_para_recortar` devolve: a imagem de teste nas medidas dadas. */
  const escolherNoDuble = async (lado, medidas, animada = false) => {
    await page.evaluate(([medidas, animada]) => {
      const [l, a] = medidas;
      // A prévia do Rust cabe em 2048: a do dublê também.
      const escala = Math.min(1, 2048 / Math.max(l, a));
      testTable.alca = (testTable.alca ?? 0) + 1;
      testTable.escolher_imagem_para_recortar = () => ({
        escolheu: true,
        alca: testTable.alca,
        previa: bancadaDoRecorte.imagemDeTesteEmUri(Math.max(1, Math.round(l * escala)), Math.max(1, Math.round(a * escala))),
        largura: l,
        altura: a,
        animada,
      });
      chamadas.length = 0;
    }, [medidas, animada]);
    await page.click(lado === "Capa" ? "#perfil-capa-escolher" : "#perfil-foto-escolher");
    await esperarORecorte(page);
  };
  const chamada = (comando) => page.evaluate((comando) => chamadas.filter((c) => c.comando === comando).map((c) => c.args), comando);
  /** O que `enviar_imagem_do_perfil` recebeu, depois de o bloco do lado dizer que enviou. */
  const mandado = async (lado) => {
    const id = lado === "Capa" ? "perfil-capa-estado" : "perfil-foto-estado";
    await page.waitForFunction((id) => ["feito", "erro"].includes(document.getElementById(id).dataset.tom), id);
    return (await chamada("enviar_imagem_do_perfil"))[0];
  };

  /** O pedaço mandado é o da moldura, em pixels inteiros da imagem inteira e na proporção exata. */
  const conferirOPedaco = async (lado, [l, a], vista, caso) => {
    const args = await mandado(lado);
    assert.ok(args, `${caso}: o CONFIRMAR não mandou a imagem`);
    const { recorte } = args;
    assert.equal(args.lado, lado);
    assert.equal(args.alca, await page.evaluate(() => testTable.alca), `${caso}: a alça não é a da imagem escolhida`);
    for (const campo of ["x", "y", "largura", "altura"]) assert.ok(Number.isInteger(recorte[campo]) && recorte[campo] >= 0, `${caso}: ${campo} = ${recorte[campo]}`);
    assert.ok(recorte.x + recorte.largura <= l && recorte.y + recorte.altura <= a && recorte.largura > 0 && recorte.altura > 0, `${caso}: o pedaço sai da imagem: ${JSON.stringify(recorte)}`);
    if (lado === "Foto") assert.equal(recorte.largura, recorte.altura, `${caso}: a foto não é quadrada: ${JSON.stringify(recorte)}`);
    // Na capa, a altura é a de 16:9 arredondada — a não ser quando a imagem não tem altura para isso.
    else assert.ok(Math.abs(recorte.altura - (recorte.largura * 9) / 16) <= 0.5 + 1e-9, `${caso}: a capa não é 16:9: ${JSON.stringify(recorte)}`);
    // Um pixel da imagem inteira de folga, mais o arredondamento da tela.
    const folga = (medida) => 1.5 / medida + 0.004;
    assert.ok(perto(recorte.x / l, vista.x0, folga(l)) && perto((recorte.x + recorte.largura) / l, vista.x1, folga(l))
      && perto(recorte.y / a, vista.y0, folga(a)) && perto((recorte.y + recorte.altura) / a, vista.y1, folga(a)),
    `${caso}: o pedaço ${JSON.stringify(recorte)} de ${l} × ${a} não é o da moldura ${JSON.stringify(vista)}`);
    return recorte;
  };

  const fazer = async (caso, f) => {
    try {
      await f();
    } catch (falha) {
      falhas.push(`${onde} · ${caso}: ${falha.message.split("\n")[0]}`);
      await retratar(page, `falha-aplicativo-${largura}-${caso.replace(/[^a-z0-9]+/gi, "-")}`);
      await page.evaluate(async () => {
        if (!document.getElementById("recorte").hidden) document.getElementById("recorte-cancelar").click();
        if (document.getElementById("perfil").hidden) await abrirPerfil();
      }).catch(() => {});
    }
  };

  // A foto: arrastada e aproximada, e o pedaço que o comando recebe.
  await fazer("foto", async () => {
    await escolherNoDuble("Foto", [4000, 2400]);
    await conferirOLayout(page, `${onde} foto`);
    await retratar(page, `recorte-foto-aplicativo-${largura}x${altura}`);
    await porOZoom(page, 2.4);
    await arrastar(page, 260, -140);
    const g = await geometria(page);
    conferirQueCobre(g, "foto");
    await page.click("#recorte-confirmar");
    await esperarORecorte(page, false);
    await conferirOPedaco("Foto", [4000, 2400], g.vista, "foto");
    await page.waitForFunction(() => document.getElementById("perfil-foto-estado").dataset.tom === "feito");
    assert.match(await page.textContent("#perfil-foto-estado"), /Foto enviada/);
    assert.equal((await chamada("esquecer_imagem_a_recortar")).length, 0, "confirmar não solta a imagem: quem a gasta é o envio");
  });

  // A capa, confirmada direto: o 16:9 centralizado que cobre.
  await fazer("capa", async () => {
    await escolherNoDuble("Capa", [4000, 2400]);
    const g = await conferirOLayout(page, `${onde} capa`);
    await retratar(page, `recorte-capa-aplicativo-${largura}x${altura}`);
    await page.keyboard.press("Enter");
    await esperarORecorte(page, false);
    const recorte = await conferirOPedaco("Capa", [4000, 2400], g.vista, "capa");
    assert.deepEqual(recorte, { x: 0, y: 75, largura: 4000, altura: 2250 });
  });

  // A imagem já na proporção, confirmada direto: o pedaço é a imagem inteira (e o Rust a deixa passar como antes).
  await fazer("inteira", async () => {
    await escolherNoDuble("Foto", [900, 900]);
    await page.keyboard.press("Enter");
    await esperarORecorte(page, false);
    assert.deepEqual((await mandado("Foto")).recorte, { x: 0, y: 0, largura: 900, altura: 900 });
    await escolherNoDuble("Capa", [1920, 1080]);
    await page.keyboard.press("Enter");
    await esperarORecorte(page, false);
    assert.deepEqual((await mandado("Capa")).recorte, { x: 0, y: 0, largura: 1920, altura: 1080 });
  });

  // As imagens difíceis: menor que a moldura, muito alongada, enorme. Abrem cobrindo, e nenhum gesto tira a moldura de dentro.
  for (const [nome, lado, medidas, zoomMaximo] of [
    ["menor que a moldura", "Foto", [40, 40], 1],
    ["menor que a moldura, na capa", "Capa", [64, 20], 1],
    ["alongada deitada", "Foto", [6000, 300], 300 / 96],
    ["alongada em pé", "Capa", [400, 5000], 400 / 320],
    ["8k", "Foto", [7680, 4320], 8],
  ]) {
    await fazer(nome, async () => {
      await escolherNoDuble(lado, medidas);
      const g = await conferirOLayout(page, `${onde} ${nome}`);
      assert.ok(perto(g.zoomMaximo, zoomMaximo, 0.011), `o zoom máximo de ${medidas.join(" × ")} é ${g.zoomMaximo}, e não ${zoomMaximo}`);
      assert.equal(g.zoomParado, zoomMaximo === 1, "o controle de zoom parado");
      if (zoomMaximo === 1) assert.match(g.nota, /pequena/, "a nota não diz por que não há zoom");
      if (nome === "alongada deitada" && largura === 1600) await retratar(page, "recorte-foto-alongada");
      if (nome === "menor que a moldura" && largura === 1600) await retratar(page, "recorte-foto-pequena");
      // Todos os gestos, para todos os lados: a moldura continua dentro.
      await porOZoom(page, 99);
      for (const [dx, dy] of [[5000, 5000], [-9000, 0], [0, -9000], [9000, 9000]]) {
        await arrastar(page, dx, dy);
        conferirQueCobre(await geometria(page), `${nome}, arrastada ${dx},${dy}`);
      }
      for (const tecla of ["-", "-", "-", "ArrowRight", "ArrowDown", "Shift+ArrowLeft", "+"]) {
        await page.keyboard.press(tecla);
        conferirQueCobre(await geometria(page), `${nome}, depois de ${tecla}`);
      }
      await porOZoom(page, 0);
      const minimo = await geometria(page);
      assert.equal(minimo.zoom, 1, "o zoom desce abaixo do que cobre a moldura");
      conferirQueCobre(minimo, `${nome}, no zoom mínimo`);
      await arrastar(page, -99999, -99999);
      const g2 = await geometria(page);
      await page.keyboard.press("Enter");
      await esperarORecorte(page, false);
      await conferirOPedaco(lado, medidas, g2.vista, nome);
    });
  }

  // Cancelar: nada é mandado, a imagem que esperava é solta, e o perfil continua aberto.
  await fazer("cancelar", async () => {
    await escolherNoDuble("Foto", [4000, 2400]);
    await porOZoom(page, 2);
    await page.keyboard.press("Escape");
    await esperarORecorte(page, false);
    // Mais que o quadro que o editor espera antes de mandar: se fosse mandar, já teria mandado.
    await page.waitForTimeout(300);
    assert.equal((await chamada("enviar_imagem_do_perfil")).length, 0, "cancelar mandou a imagem");
    assert.deepEqual((await chamada("esquecer_imagem_a_recortar")).map((args) => args.alca), [await page.evaluate(() => testTable.alca)],
      "cancelar não soltou a imagem que esperava");
    assert.deepEqual(await page.evaluate(() => ({
      perfil: !document.getElementById("perfil").hidden,
      descartar: !document.getElementById("perfil-fim-descartar").hidden,
      estado: document.getElementById("perfil-foto-estado").textContent,
      foco: document.activeElement?.id,
    })), { perfil: true, descartar: false, estado: "", foco: "perfil-foto-escolher" });
  });

  // A animada: a nota diz que ela continua animada.
  await fazer("animada", async () => {
    await escolherNoDuble("Foto", [600, 400], true);
    assert.match((await geometria(page)).nota, /continua animada/);
    await page.keyboard.press("Escape");
    await esperarORecorte(page, false);
  });

  assert.deepEqual(erros, [], `${onde}: a página lançou`);
  await page.close();
}

(async () => {
  const pagina = servirAPagina();
  const aplicativo = servir();
  await Promise.all([pagina, aplicativo].map((servidor) => new Promise((ok) => servidor.listen(0, "127.0.0.1", ok))));
  const browser = await engines[process.env.SEELE_BROWSER || "chromium"]().launch({ headless: true });
  const falhas = [];
  try {
    const daPagina = `http://127.0.0.1:${pagina.address().port}/`;
    const doAplicativo = `http://127.0.0.1:${aplicativo.address().port}/`;
    await naPaginaDeVerdade(browser, daPagina, "larga", TAMANHOS.largo, falhas);
    await naPaginaDeVerdade(browser, daPagina, "larga", TAMANHOS.medio, falhas);
    await naPaginaDeVerdade(browser, daPagina, "celular", TAMANHOS.celular, falhas);
    for (const tamanho of Object.values(TAMANHOS)) await noAplicativo(browser, doAplicativo, tamanho, falhas);
  } finally {
    await browser.close();
    pagina.close();
    aplicativo.close();
  }
  if (falhas.length) {
    console.error(`recorte-do-perfil: ${falhas.length} falha(s):\n${falhas.join("\n")}`);
    process.exitCode = 1;
    return;
  }
  console.log(
    "recorte-do-perfil: na página (larga a 1600×919 e 900×600, celular a 360×640, com a ponte e o .wasm) o que fica guardado é o pedaço da moldura, " +
      "na proporção e dentro do teto — confirmado direto, arrastado, aproximado, pelo teclado e pelo toque; JPEG sai JPEG, o GIF animado sai animado, " +
      "o 8K abre e sai; cancelar não muda nada. No aplicativo (1600×919, 900×600, 360×640) o comando recebe o pedaço da moldura, e a moldura nunca sai da imagem.",
  );
})().catch((falha) => {
  console.error(falha);
  process.exitCode = 1;
});
