// SEELE · Entry Plug — a tela de chamada (`#tela-chamada`).
//
// A grade de quem está no Cage: um cartão por piloto, com sincronia, estado e a
// moldura da forma de onda; e o monitor do Cage à direita. É a tela `chamada`
// do comp v2 (`.superpowers/sdd/comp-inventario.md` §6), que o inventário §15
// registrava como **ausente por inteiro**.
//
// ---- o que este arquivo não decide ----
//
// Nada. A faixa de cada sincronia chega decidida do core (`SyncBand`), quem
// está falando chega no snapshot, e as quatro ações são quatro `invoke`. Não há
// limiar, não há média calculada aqui e não há estado derivado — a mesma regra
// que o topo de `base.js` escreve para a janela inteira.
//
// ---- o que ela desenha vazio, e por quê ----
//
// Metade do que o comp põe nesta tela não tem dado por trás. A regra do
// produto é a de sempre: a moldura fica, o valor sai como travessão e o `title`
// diz o que faltaria. Cada motivo está em `SEM_DADO`, logo abaixo, com o item
// do inventário §16 que o classifica.
//
// ---- o que vem de fora ----
//
// `naoMedido`, `medido`, `SEM_MEDIDA` e `blocos` são declarados em
// `tela-sessao.js`, e este arquivo os usa em vez de escrever os seus. Os
// scripts dividem um escopo global (ADR 0019, e o bloco de abertura de
// `base.js`), e `tela-fim.js` já faz o mesmo com `desenhado` e `linhaAberta`.
// Duas cópias de "o que se escreve onde não há número" é uma que vai divergir —
// e é justamente a convenção que não pode divergir entre duas telas.

"use strict";

/**
 * O que falta para cada campo que esta tela desenha vazio.
 *
 * O motivo vai para o `title` do travessão: ele responde a uma pergunta que só
 * quem reparou na ausência está fazendo. Ficam juntos aqui, e não espalhados
 * pelas funções, porque são a lista do que esta tela **não** pode afirmar — e
 * uma lista que se lê de uma vez é uma lista que alguém confere contra o
 * inventário no dia em que o protocolo crescer.
 */
const SEM_DADO = {
  plug: "o protocolo não carrega quando o plug entrou; só a tela de operação teria onde começar a contar",
  rota: "o protocolo não tem o conceito de rota, nem por Dogma nem por piloto",
  tag: "o protocolo não diz qual subsistema atende cada piloto",
  onda: "a amplitude por piloto não atravessa a fronteira; o nível medido é só o de entrada desta máquina",
  volume: "o volume por pessoa é escrito e nunca lido de volta; quem o ajusta é o deslizante do roster",
  eventos: "não há histórico de entrada, saída nem A.T. Field em lugar nenhum do core",
  amostragem: "a taxa de amostragem não atravessa a fronteira",
};

/** Quantos blocos a barra de sincronia do cartão tem. 24, como no comp. */
const BLOCOS_DA_BARRA = 24;

// ------------------------------------------------------------------- desenho

/**
 * A tela inteira, a partir de um snapshot — ou da falta de um.
 *
 * `null` é o estado normal desta tela sem sessão, e não um erro: ela é
 * alcançável só de dentro da operação, mas a sessão pode acabar com ela aberta.
 */
function desenharChamada(snapshot) {
  const cage = snapshot ? snapshot.cages.find((c) => c.occupied_by_us) : null;

  desenharBarraDaChamada(snapshot, cage);
  desenharCartoes(snapshot, cage);
  desenharMonitor(snapshot, cage);
}

/** A barra de cima: a cartela do Cage, o cronômetro do plug e a rota. */
function desenharBarraDaChamada(snapshot, cage) {
  const nome = $("chamada-nome");
  if (cage) {
    medido(nome, cage.name);
  } else {
    // Não é um Cage sem nome: é a ausência de Cage. A cartela laranja em volta
    // de um travessão pareceria um Cage chamado `——`, então ela vira contorno
    // (`.chamada-cartela.ausente`).
    naoMedido(nome, snapshot ? "nenhum plug inserido" : "sem sessão");
  }

  // **L** no inventário — tempo desde o `insert_plug`, que é da casca. Mas o
  // relógio teria que nascer no instante da inserção, e a inserção acontece na
  // tela de operação: um cronômetro que começasse a contar ao abrir esta tela
  // diria que o plug entrou agora. Quando `comecoDaSessao` ganhar um irmão em
  // `tela-sessao.js`, este campo passa a ter de onde ler.
  naoMedido($("chamada-plug"), SEM_DADO.plug);

  // `BALTHASAR·01 NOMINAL` no comp, e o conceito não existe (§16).
  naoMedido($("chamada-rota"), SEM_DADO.rota);
}

/** Um cartão por piloto, ou a frase do vazio. */
function desenharCartoes(snapshot, cage) {
  const grade = $("chamada-grade");

  if (!cage || cage.pilots.length === 0) {
    const frase = !snapshot
      ? "SEM SESSÃO"
      : cage
        ? "ESTE CAGE ESTÁ VAZIO"
        : "NENHUM PLUG INSERIDO";
    repovoar(grade, [elemento("li", "chamada-vazio", frase)]);
    return;
  }

  repovoar(grade, cage.pilots.map(cartaoDoPiloto));
}

/**
 * O cartão de um piloto.
 *
 * Três faixas de informação e todas com acompanhante textual: o número ao lado
 * da marca de bloco, a barra de 24 blocos, e a etiqueta de estado por extenso.
 * O halo laranja de quem transmite é a quarta, e é a única puramente cromática
 * do comp — por isso a etiqueta diz `TRANSMITINDO` em palavra
 * (`specs/05-cliente-tui.md`).
 */
function cartaoDoPiloto(piloto) {
  const cartao = elemento("li", piloto.speaking ? "chamada-cartao falando" : "chamada-cartao");
  cartao.dataset.faixa = piloto.sync_band;

  // ---- topo: nome, tag e estado ----
  const topo = elemento("div", "cartao-topo");
  const identidade = elemento("div", "cartao-identidade");
  identidade.append(
    elemento("span", "cartao-nome", piloto.nickname + (piloto.is_self ? " (você)" : "")),
  );

  const tag = elemento("span", "cartao-tag");
  naoMedido(tag, SEM_DADO.tag);
  identidade.append(tag);

  // A mesma escada do roster, e de propósito: quem lê as duas telas lê a mesma
  // palavra para o mesmo fato. A.T. Field cala a captura, então ele vem antes
  // de "transmitindo" — não dá para estar nos dois.
  const estado = piloto.at_field
    ? "A.T. FIELD ATIVO"
    : piloto.speaking
      ? "TRANSMITINDO"
      : "EM ESCUTA";
  const etiqueta = elemento("span", "cartao-estado", estado);
  etiqueta.dataset.estado = piloto.at_field ? "at" : piloto.speaking ? "fala" : "escuta";
  topo.append(identidade, etiqueta);

  // ---- meio: a forma de onda que o protocolo não carrega ----
  const onda = elemento("div", "cartao-onda");
  const semOnda = elemento("span", null);
  naoMedido(semOnda, SEM_DADO.onda);
  onda.append(semOnda);

  // ---- base: sincronia, barra e volume ----
  const base = elemento("div", "cartao-base");

  const sync = elemento("div", "cartao-sync");
  sync.append(elemento("span", "rotulo", "SINCRONIZAÇÃO"));
  const numero = elemento("span", "cartao-numero");
  numero.append(
    elemento("span", "cartao-marca", marcaSync(piloto.sync_band)),
    // Inteiro, e não `98.4`: `sync_ratio` é `u8` em todo ponto onde existe, e a
    // casa decimal do comp seria precisão inventada no último passo.
    elemento("span", "cartao-valor", String(piloto.sync_ratio)),
  );
  sync.append(numero);

  const barra = elemento("span", "cartao-barra", blocos(piloto.sync_ratio, BLOCOS_DA_BARRA));
  barra.setAttribute("aria-hidden", "true");

  const vol = elemento("div", "cartao-vol");
  vol.append(elemento("span", "rotulo", "VOL"));
  const semVolume = elemento("span", null);
  naoMedido(semVolume, SEM_DADO.volume);
  vol.append(semVolume);

  base.append(sync, barra, vol);
  cartao.append(topo, onda, base);
  return cartao;
}

/** A coluna da direita: quem fala, os eventos e a telemetria do Cage. */
function desenharMonitor(snapshot, cage) {
  const falando = $("chamada-falando-nome");
  const nomes = cage ? cage.pilots.filter((p) => p.speaking).map((p) => p.nickname) : [];

  if (!cage) {
    naoMedido(falando, snapshot ? "nenhum plug inserido" : "sem sessão");
  } else {
    // `NINGUÉM` e não travessão: um Cage em silêncio é uma medida, e o
    // travessão está reservado para o que este protocolo não sabe dizer.
    medido(falando, nomes.length === 0 ? "NINGUÉM" : nomes.join(" · "));
  }

  const tel = snapshot ? snapshot.telemetry : null;
  if (!tel) {
    naoMedido($("chamada-atraso"), "sem sessão");
    naoMedido($("chamada-disturbio"), "sem sessão");
    naoMedido($("chamada-codec"), "sem sessão");
    return;
  }

  medido($("chamada-atraso"), `${Math.round(tel.rtt_ms)}ms`);
  medido($("chamada-disturbio"), `${(tel.loss_fraction * 100).toFixed(1)}%`);
  medido(
    $("chamada-codec"),
    snapshot.audio_available ? `OPUS ${Math.round(tel.bitrate_bps / 1000)}k` : "SEM ÁUDIO",
  );
}

/** Os dois botões de estado da barra de ações. */
function desenharAcoesDaChamada(snapshot) {
  const at = $("chamada-at");
  at.textContent = snapshot?.at_field ? "A.T. FIELD ATIVO" : "A.T. FIELD INATIVO";
  at.dataset.ativo = snapshot?.at_field ? "sim" : "nao";
  at.disabled = !snapshot;

  const surdez = $("chamada-surdez");
  surdez.textContent = snapshot?.total_isolation ? "SURDEZ TOTAL ATIVA" : "SURDEZ TOTAL";
  surdez.dataset.ativo = snapshot?.total_isolation ? "sim" : "nao";
  surdez.disabled = !snapshot;

  $("chamada-ejetar").disabled = !snapshot;
}

// ---------------------------------------------------------------- navegação

/** Abre a chamada por cima da operação. */
async function abrirChamada() {
  $("tela-sessao").hidden = true;
  $("tela-chamada").hidden = false;
  await atualizarChamada();
}

/** Fecha e devolve para a operação, que é a única porta de entrada desta tela. */
function fecharChamada() {
  $("tela-chamada").hidden = true;
  $("tela-sessao").hidden = false;
  // A sessão não foi redesenhada enquanto esteve por baixo — `desenharMensagens`
  // sai cedo quando a lista está sem layout, porque ali não dá para saber se a
  // pessoa estava lendo o histórico ou acompanhando o fim. Redesenhar agora é o
  // outro lado desse acordo, e sem ele a Linha volta parada no instante em que
  // a chamada abriu.
  atualizar().catch((falha) => console.warn("voltar da chamada:", falha));
}

/**
 * Some sem devolver ninguém.
 *
 * A sessão pode acabar com esta tela aberta — um operador derruba, o enlace
 * descarrega — e `mostrarFim` escolhe a tela seguinte por conta própria.
 * Devolver para a operação ali reabriria uma sessão que acabou de terminar; não
 * fechar deixaria duas telas empilhadas, porque toda `.tela` tem a altura da
 * janela. Mesma razão e mesma forma que `abandonarDogma`.
 */
function abandonarChamada() {
  $("tela-chamada").hidden = true;
}

/**
 * Puxa o snapshot e redesenha.
 *
 * Sem sessão isto falha, e falhar não é aviso de nada: a tela desenha o vazio e
 * espera o `mostrarFim` que já está a caminho.
 */
async function atualizarChamada() {
  let snapshot = null;
  try {
    snapshot = await invoke("snapshot");
  } catch (falha) {
    if (falha !== "NotConnected") console.warn("snapshot:", falha);
  }
  desenharChamada(snapshot);
  desenharAcoesDaChamada(snapshot);
}

// ------------------------------------------------------------------- ligação

$("botao-chamada").addEventListener("click", abrirChamada);
$("chamada-voltar").addEventListener("click", fecharChamada);

$("chamada-at").addEventListener("click", async () => {
  const snapshot = await invoke("snapshot");
  await invoke("set_at_field", { on: !snapshot.at_field });
  await atualizarChamada();
});

$("chamada-surdez").addEventListener("click", async () => {
  const snapshot = await invoke("snapshot");
  await invoke("set_total_isolation", { on: !snapshot.total_isolation });
  await atualizarChamada();
});

// Ejetar tira o plug e devolve para a Linha: sem plug esta tela não tem grade
// nenhuma para desenhar, e ficar nela mostraria o vazio de uma sala que a
// pessoa acabou de deixar de propósito.
$("chamada-ejetar").addEventListener("click", async () => {
  try {
    await invoke("eject_plug");
  } catch (falha) {
    console.warn("eject_plug:", falha);
  }
  fecharChamada();
  await atualizar();
});

// Escape fecha, que é o que uma tela alcançada de dentro de outra faz. Só com
// ela na frente, ou engoliria a tecla de quem está fechando uma busca.
window.addEventListener("keydown", (evento) => {
  if (evento.key === "Escape" && !$("tela-chamada").hidden) {
    evento.preventDefault();
    fecharChamada();
  }
});

// Quem fala muda a cada instante, e é a coisa viva desta tela. Meio segundo, o
// mesmo da telemetria da operação, e só com a tela na frente — a operação
// desliga o laço dela pela mesma condição, então as duas nunca puxam o snapshot
// ao mesmo tempo.
setInterval(() => {
  if (!$("tela-chamada").hidden) atualizarChamada();
}, 500);
