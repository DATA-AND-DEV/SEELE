// SEELE — a faixa do toque: quem está ligando para a sua sala, ATENDER e
// RECUSAR (tarefas 24, 26 e 27 da F7; spec de 23/09, «O toque» e «Atender, nesta
// ordem»).
//
// **O contrato é o da faixa de quem bate à porta** (`camada-portaria.js`,
// `avisarQueBatem`): sem `role=alert`, sem `autofocus`, **nunca** `focus()`, e
// dita uma vez por aparição pelo `anunciar`, que é a região viva que a janela já
// tem. O toque chama; quem está no meio de uma frase, de um jogo ou de uma
// apresentação decide se olha. O Rust pede atenção ao sistema e não põe a janela
// na frente (`src/toque.rs`).
//
// **Núcleo de confiança** (spec de 23/09, Parte II): a faixa mora fora das
// telas, e nenhuma região de MOD a cobre; nenhum evento de MOD fica sabendo
// dela, porque os eventos da tela saem do snapshot (`avisarOsModsDaTela`), e o
// toque nunca entra nele. Um MOD que desenhasse um toque falso levaria a pessoa a
// atender o que não devia.
//
// **O navegador não tem sala.** A faixa leva `data-sala-pessoal`, que as duas
// cascas do navegador escondem (F8), e toda chamada daqui aceita a recusa da
// ponte do navegador sem estourar e sem esperar para sempre.

"use strict";

/** Quem toca agora, pela impressão da chave, na ordem em que chegou. */
const tocandoNaFaixa = new Map();

/**
 * O ATENDER que não chegou à sala: `null`, ou `{ alvo, tentativa }`. Mantém a
 * faixa à vista, com TENTAR DE NOVO, mesmo com o toque já parado como atendido:
 * quem ligava está dentro, e quem recebe ainda não (spec, «Atender, nesta
 * ordem», 4).
 */
let atendimentoFalho = null;

/** A falha dupla encerrou a ligação: a faixa diz isso até alguém fechá-la. */
let ligacaoEncerrada = false;

/** Um ATENDER em curso: os botões não disparam outro por cima. */
let atendendo = false;

/** Uma frase de recusa sob o texto da faixa, ou `null`. */
let notaDoToque = null;

/**
 * **A faixa empurra a tela, e não a cobre** (medido pela F3, 15/10; a regra
 * mora em `camada-toque.css`). Acesa, ela diz à raiz até onde vai
 * (`--toque-ocupa`), e a tela à vista abre esse espaço no topo; apagada, a tela
 * volta ao lugar.
 *
 * Medida aqui, com `getBoundingClientRect`, a cada vez que a faixa acende ou
 * apaga: o `ResizeObserver` de baixo só avisa no passo de desenho, que o
 * navegador sem cabeça dos roteiros não roda. Ele fica para quando a faixa muda
 * de altura sozinha, com a janela que estreita e quebra o texto em duas linhas.
 */
function abrirEspacoParaAFaixa() {
  const raiz = document.documentElement;
  const faixa = $("toque");
  if (faixa.hidden) {
    raiz.removeAttribute("data-toque-aceso");
    raiz.style.removeProperty("--toque-ocupa");
    return;
  }
  raiz.style.setProperty("--toque-ocupa", `${Math.ceil(faixa.getBoundingClientRect().bottom)}px`);
  raiz.setAttribute("data-toque-aceso", "");
}

new ResizeObserver(() => abrirEspacoParaAFaixa()).observe($("toque"));

/** Quem a faixa mostra: o primeiro que ainda toca. */
function quemNaFaixa() {
  return tocandoNaFaixa.values().next().value ?? null;
}

/**
 * Desenha a faixa a partir do que se sabe agora, e a diz em voz alta **uma vez
 * por aparição**: é a transição para a vista que merece a frase, e não cada
 * redesenho.
 */
async function desenharToque() {
  const faixa = $("toque");
  const texto = $("toque-texto");
  const nota = $("toque-nota");
  const atender = $("toque-atender");
  const recusar = $("toque-recusar");
  const tocando = quemNaFaixa();

  let frase;
  let rotuloDoAtender = "ATENDER";
  let rotuloDoRecusar = "RECUSAR";
  let comNota = notaDoToque;
  if (ligacaoEncerrada) {
    frase = SALA_PESSOAL.encerrada;
    rotuloDoAtender = null;
    rotuloDoRecusar = "FECHAR";
  } else if (atendimentoFalho) {
    frase = SALA_PESSOAL.naoEntrou;
    rotuloDoAtender = SALA_PESSOAL.tentarDeNovo;
    rotuloDoRecusar = "ENCERRAR";
  } else if (tocando) {
    frase = SALA_PESSOAL.ligando(tocando);
    if (tocando.com_som === false) comNota ??= SALA_PESSOAL.semSom;
    // «Atender sai de Y»: quem está num servidor sai dele para entrar na sala.
    // O próprio servidor continua no ar: sair não derruba nada (`ejetar`).
    const onde = await invoke("onde_estou").catch(() => null);
    if (onde === "Guardado" || onde === "Outro") {
      rotuloDoAtender = SALA_PESSOAL.atenderSaiDe(nomeDesteServidor());
    }
  } else {
    notaDoToque = null;
    faixa.hidden = true;
    abrirEspacoParaAFaixa();
    return;
  }

  texto.textContent = frase;
  nota.textContent = comNota ?? "";
  nota.hidden = !comNota;
  atender.hidden = rotuloDoAtender === null;
  if (rotuloDoAtender !== null) atender.textContent = rotuloDoAtender;
  recusar.textContent = rotuloDoRecusar;
  atender.disabled = atendendo;
  recusar.disabled = atendendo;

  // Antes de escrever o `hidden`, porque a pergunta é se ela **estava**
  // escondida.
  if (faixa.hidden) anunciar(`${frase}.`);
  faixa.hidden = false;
  abrirEspacoParaAFaixa();
}

/** Espera um `conectar` em voo terminar, até dez segundos. */
async function esperarOConectarEmVoo() {
  for (let esperado = 0; esperado < 10000 && $("botao-conectar").disabled; esperado += 100) {
    await new Promise((pronto) => setTimeout(pronto, 100));
  }
}

/** Senta na sala de voz da ligação: a sala tem uma só. */
async function sentarNaSalaDaLigacao() {
  try {
    const quadro = await invoke("snapshot");
    const sala = quadro?.voice_rooms?.[0];
    if (sala) await invoke("enter_voice_room", { voiceRoom: sala.id, senha: null });
  } catch (falha) {
    console.warn("sentar na sala da ligação:", falha);
  }
}

/** A falha dupla: ninguém fica sozinho na sala. */
async function encerrarALigacao() {
  try {
    await invoke("encerrar_a_ligacao");
  } catch (falha) {
    console.warn("encerrar_a_ligacao:", falha);
  }
  atendimentoFalho = null;
  ligacaoEncerrada = true;
}

/**
 * Entra na sala depois de atender (spec, «Atender, nesta ordem», 3): espera um
 * `conectar` em voo, leva à entrada **sem ejetar** nenhuma sessão (desde a
 * 0.17 as sessões somam), entra na sala
 * pelo `conectar` de sempre (o Rust confere a sala pelo código desta máquina) e
 * senta na sala de voz. Quem responde se entrou é o `onde_estou`, e não a tela.
 */
async function entrarNaSalaAoAtender(alvo, tentativa) {
  atendendo = true;
  await desenharToque();
  try {
    await esperarOConectarEmVoo();
    // **Atender soma uma sessão, e não troca** (E3 da 0.17): com um servidor
    // aberto, `ejetar` o derrubaria. Vai à entrada sem sair de nenhuma, e o
    // `conectar` de sempre abre a da ligação; se ele falhar, a frase aparece
    // na entrada, e as outras sessões continuam na lista.
    if (!$("tela-sessao").hidden) irParaAEntradaSemSair();
    await conectar(alvo);
    const onde = await invoke("onde_estou").catch(() => null);
    if (onde === "Pessoal") {
      atendimentoFalho = null;
      await sentarNaSalaDaLigacao();
      return;
    }
    if (tentativa < 2) {
      atendimentoFalho = { alvo, tentativa };
      return;
    }
    await encerrarALigacao();
  } finally {
    atendendo = false;
    await desenharToque();
  }
}

async function atenderOToque() {
  if (atendendo) return;
  if (atendimentoFalho) {
    await entrarNaSalaAoAtender(atendimentoFalho.alvo, atendimentoFalho.tentativa + 1);
    return;
  }
  const tocando = quemNaFaixa();
  if (!tocando) return;
  let atendido;
  try {
    atendido = await invoke("atender", { quem: tocando.quem });
  } catch (falha) {
    notaDoToque = SALA_PESSOAL.aoAtender[falha] ?? null;
    await desenharToque();
    return;
  }
  notaDoToque = null;
  if (!atendido || atendido.ja_na_sala) return;
  await entrarNaSalaAoAtender(atendido.alvo, 1);
}

async function recusarOToque() {
  if (atendendo) return;
  if (ligacaoEncerrada) {
    ligacaoEncerrada = false;
    await desenharToque();
    return;
  }
  if (atendimentoFalho) {
    await encerrarALigacao();
    await desenharToque();
    return;
  }
  const tocando = quemNaFaixa();
  if (!tocando) return;
  try {
    await invoke("recusar", { quem: tocando.quem });
  } catch (falha) {
    notaDoToque = SALA_PESSOAL.aoAtender[falha] ?? null;
    await desenharToque();
  }
}

$("toque-atender").addEventListener("click", () => {
  atenderOToque().catch((falha) => console.warn("ATENDER:", falha));
});
$("toque-recusar").addEventListener("click", () => {
  recusarOToque().catch((falha) => console.warn("RECUSAR:", falha));
});

listen("seele://toque", ({ payload }) => {
  if (payload?.evento === "Tocou") {
    tocandoNaFaixa.set(payload.quem, payload);
    ligacaoEncerrada = false;
  } else if (payload?.evento === "Parou") {
    tocandoNaFaixa.delete(payload.quem);
    if (tocandoNaFaixa.size === 0 && !atendimentoFalho && !ligacaoEncerrada) {
      const frase = SALA_PESSOAL.desfecho[payload.desfecho];
      if (frase) anunciar(`${frase}.`);
    }
  }
  desenharToque().catch((falha) => console.warn("faixa do toque:", falha));
}).catch((falha) => console.warn("seele://toque:", falha));

// A tela que abre com o toque já tocando. No navegador a ponte recusa, e a faixa
// fica escondida.
invoke("toques_agora")
  .then((lista) => {
    for (const tocando of lista ?? []) tocandoNaFaixa.set(tocando.quem, tocando);
    return desenharToque();
  })
  .catch(() => {});
