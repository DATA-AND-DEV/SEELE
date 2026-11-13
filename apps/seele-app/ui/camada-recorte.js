// SEELE — o passo de recorte da foto e da capa do perfil (`#recorte`, 1.0.1).
//
// **O pedido do dono**: «às vezes a imagem é muito maior, um wallpaper, e eu
// quero uma parte específica. Deveria ter um crop: dar zoom na imagem e
// escolher a parte dela, no formato da imagem (a foto normalmente quadrada). E
// isso para a capa também».
//
// Escolher a foto ou a capa virou dois passos. `escolher_imagem_para_recortar`
// abre o seletor e devolve a **prévia** da imagem (dentro de um teto) e as
// medidas da imagem inteira; esta camada mostra a prévia atrás de uma moldura
// na proporção de destino, e a pessoa arrasta e aproxima até a moldura pegar o
// pedaço que ela quer. CONFIRMAR devolve o pedaço **em pixels da imagem
// inteira**, e quem recorta é o Rust (`reducao::imagem_do_perfil_recortada`),
// sobre os bytes que ficaram do lado de lá — a mesma conta no aplicativo e no
// navegador, que segue pelo teto e pelo envio de sempre.
//
// # O que esta camada não faz
//
// Não recorta. Um canvas aqui congelaria um GIF animado no primeiro quadro,
// regravaria um JPEG em PNG e cortaria da prévia, e não da imagem. Ela só
// mede: onde a moldura está sobre a imagem.
//
// # Quem não quer recortar
//
// Confirma direto: o recorte inicial é o centralizado que cobre a moldura — o
// mesmo pedaço que o produto já mostrava de uma imagem fora da proporção.
//
// # O teclado
//
// Com o foco na área da imagem, as setas a movem (Shift, em passos maiores);
// `+` e `−` mudam o zoom em qualquer lugar do diálogo; Enter confirma, Esc
// cancela, e o Tab não sai da camada. Os ouvintes são de captura na janela, e
// este script carrega antes dos das outras camadas: com o recorte aberto, ele
// é a camada de cima, e a tecla não chega ao «Seu perfil» atrás dele.

"use strict";

/**
 * As molduras, pelo lado: a proporção de destino, e a menor largura (em pixels
 * da imagem) que o zoom deixa escolher — abaixo disso o que sobe é um borrão.
 * A foto é quadrada; a capa é 16:9, a proporção da amostra do editor e do teto
 * em que ela é guardada (1280 × 720).
 */
const MOLDURAS_DO_RECORTE = Object.freeze({
  Foto: Object.freeze({ largura: 1, altura: 1, menor: 96 }),
  Capa: Object.freeze({ largura: 16, altura: 9, menor: 320 }),
});

/** Até onde o zoom vai, quando a imagem tem pixels para isso. */
const ZOOM_MAXIMO_DO_RECORTE = 8;

/** Quanto cada `+`/`−` (e cada botão) multiplica o zoom. */
const PASSO_DO_ZOOM_DO_RECORTE = 1.15;

/** Quanto da moldura cada seta anda; com Shift, o quádruplo. */
const PASSO_DAS_SETAS_DO_RECORTE = 0.05;

/**
 * O recorte aberto, ou `null`: o lado, as medidas da imagem inteira, o zoom
 * (1 é o que cobre a moldura), o centro da moldura sobre a imagem (`cx`, `cy`,
 * de 0 a 1) e a moldura e o palco em pixels da tela.
 */
let recorteAberto = null;

/** O maior zoom de uma imagem: até a moldura pegar só `menor` pixels dela. */
function zoomMaximoDoRecorte(lado, largura, altura) {
  const moldura = MOLDURAS_DO_RECORTE[lado] ?? MOLDURAS_DO_RECORTE.Foto;
  // A largura do pedaço com o zoom em 1: a imagem inteira num dos eixos.
  const cobrindo = Math.min(largura, (altura * moldura.largura) / moldura.altura);
  return Math.max(1, Math.min(ZOOM_MAXIMO_DO_RECORTE, cobrindo / moldura.menor));
}

/**
 * **O pedaço que a moldura pega, em pixels inteiros da imagem inteira**, na
 * proporção exata do lado (a foto, quadrada; a capa, 16:9) e dentro da imagem.
 * Com o zoom em 1 e a imagem já na proporção, é a imagem inteira — e o Rust a
 * deixa passar como passava antes do recorte.
 */
function pedacoDoRecorte({ lado, largura, altura, zoom, cx, cy }) {
  const moldura = MOLDURAS_DO_RECORTE[lado] ?? MOLDURAS_DO_RECORTE.Foto;
  const proporcao = moldura.largura / moldura.altura;
  const cobrindo = Math.min(largura, altura * proporcao);
  let larguraDoPedaco = Math.max(1, Math.min(largura, Math.round(cobrindo / zoom)));
  let alturaDoPedaco = Math.max(1, Math.round(larguraDoPedaco / proporcao));
  if (alturaDoPedaco > altura) {
    alturaDoPedaco = altura;
    larguraDoPedaco = Math.max(1, Math.min(largura, Math.round(altura * proporcao)));
  }
  const dentro = (valor, maximo) => Math.max(0, Math.min(maximo, Math.round(valor)));
  return {
    x: dentro(cx * largura - larguraDoPedaco / 2, largura - larguraDoPedaco),
    y: dentro(cy * altura - alturaDoPedaco / 2, altura - alturaDoPedaco),
    largura: larguraDoPedaco,
    altura: alturaDoPedaco,
  };
}

/** A imagem na tela: o tamanho com o zoom de agora, em pixels da tela. */
function imagemDoRecorteNaTela(r) {
  const escala = Math.max(r.moldura.largura / r.largura, r.moldura.altura / r.altura) * r.zoom;
  return { largura: r.largura * escala, altura: r.altura * escala };
}

/** Segura o zoom e o centro dentro do que a imagem tem: a moldura nunca sai dela. */
function conterORecorte(r) {
  r.zoom = Math.max(1, Math.min(r.zoomMaximo, r.zoom));
  const naTela = imagemDoRecorteNaTela(r);
  const meiaX = Math.min(0.5, r.moldura.largura / (2 * naTela.largura));
  const meiaY = Math.min(0.5, r.moldura.altura / (2 * naTela.altura));
  r.cx = Math.max(meiaX, Math.min(1 - meiaX, r.cx));
  r.cy = Math.max(meiaY, Math.min(1 - meiaY, r.cy));
}

/**
 * Mede o palco e põe a moldura no meio dele, na proporção do lado e com
 * margem: o que sobra em volta é a imagem escurecida, que é como se vê o que
 * ficou de fora.
 */
function medirOPalcoDoRecorte() {
  const r = recorteAberto;
  if (!r) return;
  const palco = $("recorte-palco");
  const largura = palco.clientWidth;
  const altura = palco.clientHeight;
  if (largura <= 0 || altura <= 0) return;
  const molde = MOLDURAS_DO_RECORTE[r.lado];
  const proporcao = molde.largura / molde.altura;
  const margem = largura < 360 ? 16 : 24;
  const larguraDaMoldura = Math.max(48, Math.floor(Math.min(largura - 2 * margem, (altura - 2 * margem) * proporcao)));
  const alturaDaMoldura = Math.max(27, Math.floor(larguraDaMoldura / proporcao));
  r.palco = { largura, altura };
  r.moldura = { largura: larguraDaMoldura, altura: alturaDaMoldura };
  const moldura = $("recorte-moldura");
  moldura.style.width = `${larguraDaMoldura}px`;
  moldura.style.height = `${alturaDaMoldura}px`;
  moldura.style.left = `${(largura - larguraDaMoldura) / 2}px`;
  moldura.style.top = `${(altura - alturaDaMoldura) / 2}px`;
  desenharORecorte();
}

/** Como o zoom é lido: «1,5×» na tela, «1,5 vezes» para o leitor de tela. */
function zoomDoRecorteEscrito(zoom) {
  return zoom.toLocaleString("pt-BR", { minimumFractionDigits: 1, maximumFractionDigits: 1 });
}

/** Põe a imagem atrás da moldura, e o controle de zoom no valor de agora. */
function desenharORecorte() {
  const r = recorteAberto;
  if (!r || !r.moldura) return;
  conterORecorte(r);
  const naTela = imagemDoRecorteNaTela(r);
  const imagem = $("recorte-imagem");
  imagem.style.width = `${naTela.largura}px`;
  imagem.style.height = `${naTela.altura}px`;
  const esquerda = r.palco.largura / 2 - r.cx * naTela.largura;
  const topo = r.palco.altura / 2 - r.cy * naTela.altura;
  imagem.style.transform = `translate(${esquerda}px, ${topo}px)`;
  const f = FRASES_DOS_PERFIS.recorte;
  const zoom = $("recorte-zoom");
  zoom.value = String(r.zoom);
  zoom.setAttribute("aria-valuetext", f.zoomFalado(zoomDoRecorteEscrito(r.zoom)));
  $("recorte-zoom-valor").textContent = f.zoomValor(zoomDoRecorteEscrito(r.zoom));
  $("recorte-menos").disabled = r.zoom <= 1;
  $("recorte-mais").disabled = r.zoom >= r.zoomMaximo;
}

/** Anda com a imagem `dx`, `dy` pixels da tela (arrastar para a direita mostra o que estava à esquerda). */
function moverORecorte(dx, dy) {
  const r = recorteAberto;
  if (!r || !r.moldura) return;
  const naTela = imagemDoRecorteNaTela(r);
  r.cx -= dx / naTela.largura;
  r.cy -= dy / naTela.altura;
  desenharORecorte();
}

/** Multiplica o zoom, em volta do centro da moldura. */
function aproximarORecorte(fator) {
  const r = recorteAberto;
  if (!r || !Number.isFinite(fator) || fator <= 0) return;
  r.zoom *= fator;
  desenharORecorte();
}

/** Diz ao leitor de tela onde a moldura ficou, depois de uma tecla. */
let avisoDaPosicaoDoRecorte = 0;
function dizerAPosicaoDoRecorte() {
  clearTimeout(avisoDaPosicaoDoRecorte);
  avisoDaPosicaoDoRecorte = setTimeout(() => {
    const r = recorteAberto;
    if (!r || !r.moldura) return;
    const naTela = imagemDoRecorteNaTela(r);
    const meiaX = Math.min(0.5, r.moldura.largura / (2 * naTela.largura));
    const meiaY = Math.min(0.5, r.moldura.altura / (2 * naTela.altura));
    const parte = (centro, meia) => (meia >= 0.5 ? 50 : Math.round(((centro - meia) / (1 - 2 * meia)) * 100));
    anunciar(FRASES_DOS_PERFIS.recorte.posicao(parte(r.cx, meiaX), parte(r.cy, meiaY), zoomDoRecorteEscrito(r.zoom)));
  }, 350);
}

/**
 * **Abre o passo de recorte** sobre o que `escolher_imagem_para_recortar`
 * devolveu, e espera a pessoa. Resolve com o pedaço `{ x, y, largura, altura }`
 * em pixels da imagem inteira (CONFIRMAR, Enter), ou com `null` (CANCELAR, Esc,
 * apertar fora, o voltar do aparelho): nada muda, e quem chamou solta a imagem.
 */
function recortarAImagem({ lado, previa, largura, altura, animada }) {
  // Um recorte por vez: abrir outro cancela o que estava aberto.
  if (recorteAberto) fecharORecorte(null);
  const f = FRASES_DOS_PERFIS.recorte;
  const ladoDaMoldura = lado === "Capa" ? "Capa" : "Foto";
  return new Promise((responder) => {
    recorteAberto = {
      lado: ladoDaMoldura,
      largura: Math.max(1, Number(largura) || 1),
      altura: Math.max(1, Number(altura) || 1),
      zoom: 1,
      zoomMaximo: 1,
      cx: 0.5,
      cy: 0.5,
      palco: null,
      moldura: null,
      dedos: new Map(),
      responder,
      foco: document.activeElement,
      soltarDoVoltar: null,
    };
    const r = recorteAberto;
    r.zoomMaximo = zoomMaximoDoRecorte(r.lado, r.largura, r.altura);
    const raiz = $("recorte");
    raiz.dataset.lado = r.lado;
    $("recorte-titulo").textContent = f.titulo(r.lado);
    $("recorte-ajuda").textContent = f.ajuda;
    $("recorte-palco").setAttribute("aria-label", f.palco(r.lado));
    $("recorte-zoom-rotulo").textContent = f.zoom;
    $("recorte-menos").setAttribute("aria-label", f.menos);
    $("recorte-mais").setAttribute("aria-label", f.mais);
    $("recorte-cancelar").textContent = f.cancelar;
    $("recorte-confirmar").textContent = f.confirmar;
    const zoom = $("recorte-zoom");
    zoom.min = "1";
    zoom.max = String(r.zoomMaximo);
    zoom.step = "0.01";
    // A imagem pequena demais para aproximar: o controle fica, parado, e a nota diz por quê.
    zoom.disabled = r.zoomMaximo <= 1;
    const notas = [];
    if (r.zoomMaximo <= 1) notas.push(f.semZoom);
    if (animada) notas.push(f.animada);
    if (r.lado === "Capa") notas.push(f.capa);
    const nota = $("recorte-nota");
    nota.textContent = notas.join(" ");
    nota.hidden = notas.length === 0;
    const imagem = $("recorte-imagem");
    imagem.src = previa;
    raiz.hidden = false;
    medirOPalcoDoRecorte();
    $("recorte-palco").focus({ preventScroll: true });
    anunciar(f.aberto(r.lado));
    // O voltar do celular fecha a camada de cima: aqui, cancela o recorte.
    const empilhar = typeof CELULAR === "object" && CELULAR ? CELULAR.ajudante("empilhar") : null;
    r.soltarDoVoltar = typeof empilhar === "function" ? empilhar(() => fecharORecorte(null, false)) : null;
  });
}

/**
 * Fecha a camada e responde a quem a abriu: o pedaço, ou `null`. `devolverFoco`
 * é falso quando quem fecha é o voltar do aparelho, que já leva o foco.
 */
function fecharORecorte(pedaco, devolverFoco = true) {
  const r = recorteAberto;
  if (!r) return;
  recorteAberto = null;
  clearTimeout(avisoDaPosicaoDoRecorte);
  // O foco sai de dentro da camada antes de ela sumir: o navegador só o tira de
  // um controle escondido no quadro seguinte, e até lá quem pergunta «onde
  // está o teclado?» (o editor do perfil, para devolvê-lo ao ESCOLHER) ouviria
  // o nome de um botão que ninguém vê.
  const ativo = document.activeElement;
  if (ativo instanceof HTMLElement && $("recorte").contains(ativo)) ativo.blur();
  $("recorte").hidden = true;
  $("recorte-palco").dataset.arrastando = "nao";
  // A prévia é um `data:` de até alguns megabytes: fechada a camada, sai da página.
  $("recorte-imagem").removeAttribute("src");
  const soltar = r.soltarDoVoltar;
  r.soltarDoVoltar = null;
  soltar?.();
  if (devolverFoco && focavel(r.foco)) r.foco.focus();
  if (!pedaco) anunciar(FRASES_DOS_PERFIS.recorte.cancelado);
  r.responder(pedaco);
}

function confirmarORecorte() {
  const r = recorteAberto;
  if (!r) return;
  conterORecorte(r);
  fecharORecorte(pedacoDoRecorte(r));
}

function cancelarORecorte() {
  fecharORecorte(null);
}

$("recorte-confirmar").addEventListener("click", confirmarORecorte);
$("recorte-cancelar").addEventListener("click", cancelarORecorte);
$("recorte-menos").addEventListener("click", () => aproximarORecorte(1 / PASSO_DO_ZOOM_DO_RECORTE));
$("recorte-mais").addEventListener("click", () => aproximarORecorte(PASSO_DO_ZOOM_DO_RECORTE));
$("recorte-zoom").addEventListener("input", () => {
  if (!recorteAberto) return;
  recorteAberto.zoom = Number($("recorte-zoom").value) || 1;
  desenharORecorte();
});
fecharAoClicarFora("recorte", cancelarORecorte);

// A janela muda de tamanho (e o celular vira): a moldura é medida de novo, e o
// pedaço escolhido continua o mesmo, porque o estado é em frações da imagem.
if (typeof ResizeObserver === "function") {
  new ResizeObserver(() => medirOPalcoDoRecorte()).observe($("recorte-palco"));
} else {
  window.addEventListener("resize", medirOPalcoDoRecorte);
}

// ------------------------------------------------------- arrastar e a pinça
//
// Eventos de ponteiro: o mouse, a caneta e o dedo pelo mesmo caminho. Um
// ponteiro arrasta; dois fazem a pinça (a distância entre eles muda o zoom, e
// o meio deles arrasta). O `touch-action: none` da folha é o que impede o
// navegador de rolar ou ampliar a página no lugar.

$("recorte-palco").addEventListener("pointerdown", (evento) => {
  const r = recorteAberto;
  if (!r || (evento.pointerType === "mouse" && evento.button !== 0)) return;
  const palco = $("recorte-palco");
  try {
    palco.setPointerCapture(evento.pointerId);
  } catch {
    // Um ponteiro que já acabou não se captura, e o arrasto vale dentro do palco mesmo assim.
  }
  r.dedos.set(evento.pointerId, { x: evento.clientX, y: evento.clientY });
  palco.dataset.arrastando = "sim";
  palco.focus({ preventScroll: true });
  evento.preventDefault();
});

$("recorte-palco").addEventListener("pointermove", (evento) => {
  const r = recorteAberto;
  const antes = r?.dedos.get(evento.pointerId);
  if (!r || !antes) return;
  const agora = { x: evento.clientX, y: evento.clientY };
  if (r.dedos.size === 1) {
    moverORecorte(agora.x - antes.x, agora.y - antes.y);
  } else if (r.dedos.size === 2) {
    const [outro] = [...r.dedos].filter(([id]) => id !== evento.pointerId).map(([, ponto]) => ponto);
    const distancia = (a, b) => Math.hypot(a.x - b.x, a.y - b.y);
    const de = distancia(antes, outro);
    const para = distancia(agora, outro);
    if (de > 0 && para > 0) r.zoom *= para / de;
    // O meio dos dois dedos andou a metade do que este andou.
    moverORecorte((agora.x - antes.x) / 2, (agora.y - antes.y) / 2);
  }
  r.dedos.set(evento.pointerId, agora);
  evento.preventDefault();
});

for (const fim of ["pointerup", "pointercancel", "lostpointercapture"]) {
  $("recorte-palco").addEventListener(fim, (evento) => {
    const r = recorteAberto;
    if (!r) return;
    r.dedos.delete(evento.pointerId);
    if (r.dedos.size === 0) $("recorte-palco").dataset.arrastando = "nao";
  });
}

// A roda do mouse aproxima e afasta. `passive: false`: sem o `preventDefault`,
// a página atrás rolaria junto. A pinça do trackpad chega como roda com Ctrl,
// em passos pequenos.
$("recorte-palco").addEventListener(
  "wheel",
  (evento) => {
    if (!recorteAberto) return;
    evento.preventDefault();
    const linhas = evento.deltaMode === 1 ? 16 : 1;
    const forca = evento.ctrlKey ? 0.01 : 0.0018;
    aproximarORecorte(Math.exp(-evento.deltaY * linhas * forca));
  },
  { passive: false },
);

// --------------------------------------------------------------- o teclado

/** Os controles do diálogo, na ordem do Tab. */
function focaveisDoRecorte() {
  return ["recorte-palco", "recorte-menos", "recorte-zoom", "recorte-mais", "recorte-cancelar", "recorte-confirmar"]
    .map((id) => $(id))
    .filter((e) => !e.disabled && e.getClientRects().length > 0);
}

window.addEventListener(
  "keydown",
  (evento) => {
    const r = recorteAberto;
    if (!r) return;
    const parar = () => {
      evento.preventDefault();
      // Imediata: o «Seu perfil» atrás desta camada ouve o Esc e o Tab na
      // mesma janela, e a tecla é desta camada.
      evento.stopImmediatePropagation();
    };
    const raiz = $("recorte");
    const ativo = document.activeElement;
    if (evento.key === "Escape") {
      parar();
      cancelarORecorte();
      return;
    }
    if (evento.key === "Tab") {
      // O foco preso: do último volta ao primeiro, e o que escapou é trazido.
      const lista = focaveisDoRecorte();
      const posicao = lista.indexOf(ativo);
      parar();
      if (lista.length === 0) return;
      const proximo = posicao < 0 ? 0 : (posicao + (evento.shiftKey ? -1 : 1) + lista.length) % lista.length;
      lista[proximo].focus();
      return;
    }
    if (evento.ctrlKey || evento.metaKey || evento.altKey) return;
    if (evento.key === "Enter") {
      // Num botão, o Enter é do botão (CANCELAR cancela); no resto, confirma.
      if (ativo instanceof HTMLButtonElement && raiz.contains(ativo)) return;
      parar();
      confirmarORecorte();
      return;
    }
    if (evento.key === "+" || evento.key === "=") {
      parar();
      aproximarORecorte(PASSO_DO_ZOOM_DO_RECORTE);
      dizerAPosicaoDoRecorte();
      return;
    }
    if (evento.key === "-" || evento.key === "_" || evento.key === "−") {
      parar();
      aproximarORecorte(1 / PASSO_DO_ZOOM_DO_RECORTE);
      dizerAPosicaoDoRecorte();
      return;
    }
    const setas = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
    const seta = setas[evento.key];
    // As setas movem a moldura com o foco na área da imagem; no controle de
    // zoom elas são dele, como em toda régua.
    if (seta && ativo === $("recorte-palco") && r.moldura) {
      parar();
      const passo = PASSO_DAS_SETAS_DO_RECORTE * (evento.shiftKey ? 4 : 1);
      // A moldura anda para o lado da seta: a imagem, para o outro.
      moverORecorte(-seta[0] * passo * r.moldura.largura, -seta[1] * passo * r.moldura.altura);
      dizerAPosicaoDoRecorte();
    }
  },
  true,
);
