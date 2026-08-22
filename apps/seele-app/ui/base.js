// SEELE · Entry Plug — a camada comum a toda tela.
//
// Este arquivo desenha e nada mais. `specs/06-clientes-gui.md`: "Nenhuma lógica
// de protocolo em JavaScript. Se o frontend precisa saber o que é um `ssrc`,
// algo está errado." Nada aqui sabe o que é um ssrc, o que faz o sinal ser
// crítico, ou quando reconectar. Tudo isso chega decidido dentro do snapshot.
//
// O padrão é o mesmo de `seele-tui::view`: projetar o snapshot inteiro a cada
// mudança. Não há estado derivado nem cache — a tela é função de um valor que
// chega pronto. ADR 0019 explica por que isso dispensa framework.
//
// ---- o que mora aqui, e o que a divisão exige ----
//
// A ponte para o Rust, os quatro ajudantes de DOM, os formatadores puros e o
// laço que puxa o snapshot. O que só uma tela usa mora em `tela-<nome>.js`.
//
// Não há módulo nem `import` (ADR 0019): os arquivos dividem o mesmo escopo
// global, e o que muda ao dividir não é a visibilidade, é o **instante**. Uma
// função declarada em `tela-sessao.js` já existe quando qualquer coisa a chama
// de dentro de um manipulador — mas não existe enquanto `tela-boot.js` executa
// seu corpo de topo. Daí a regra que `index.html` segue e
// `apps/seele-app/tests/frontend.rs` confere:
//
//   1. `base.js` primeiro: todo o resto o usa em tempo de execução;
//   2. `glifos.js` e `frases.js` antes das telas;
//   3. cada `tela-<nome>.js` registra os **seus** ouvintes no seu próprio rodapé,
//      porque `addEventListener(…, funcao)` lê a função na hora, e não depois.
//
// `let` no topo de um script clássico é ligação de escopo de script, dividida
// entre todos eles: `tela-fim.js` escreve em `desenhado`, que `tela-sessao.js`
// declara. Isso vale porque só acontece dentro de um manipulador, muito depois
// de todos os arquivos terem rodado.

"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);

// ---------------------------------------------------------------- utilidades

/**
 * O horário local de um instante do servidor.
 *
 * A FFI entrega **segundos** — a unidade está no nome do campo porque errá-la
 * já desenhou toda mensagem como 1970 uma vez.
 */
function relogio(segundos) {
  if (!segundos) return "--:--";
  const quando = new Date(segundos * 1000);
  return quando.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/**
 * Quanto tempo faz, em palavras curtas.
 *
 * A data exata não ajuda a escolher para onde voltar; "ontem" ajuda.
 */
function quando(segundos) {
  if (!segundos) return "—";
  const dias = Math.floor((Date.now() / 1000 - segundos) / 86400);
  if (dias <= 0) return "hoje";
  if (dias === 1) return "ontem";
  return `${dias} dias`;
}

/**
 * A marca de bloco de uma faixa do sinal.
 *
 * `specs/05-cliente-tui.md`: nenhuma informação transmitida só por cor. A marca
 * é a metade que sobrevive sem cor nenhuma, e é desenhada em toda paleta — uma
 * marca que só aparece quando piora é uma marca que ninguém aprendeu a ler.
 *
 * Três entradas, as mesmas três de `seele-tui::theme` (ADR 0024). A quarta,
 * `Acceptable`, saiu com a faixa: ela não chega mais do core, e o `?? "░"` a
 * levaria para crítico — que é a leitura certa para um nome que este cliente
 * não conhece, e a errada para um que ele conhecia semana passada.
 */
function marcaSync(faixa) {
  return { Nominal: "█", Degraded: "▒", Critical: "░" }[faixa] ?? "░";
}

/** Substitui os filhos de um elemento por uma lista nova. */
function repovoar(pai, filhos) {
  pai.replaceChildren(...filhos);
}

function elemento(tag, classe, texto) {
  const nodo = document.createElement(tag);
  if (classe) nodo.className = classe;
  if (texto !== undefined) nodo.textContent = texto;
  return nodo;
}

// --------------------------------------------------------------- o snapshot

async function atualizar() {
  try {
    desenhar(await invoke("snapshot"));
  } catch (erro) {
    // Sem sessão. Não é uma falha: é o estado antes de conectar e depois de sair.
    if (erro !== "NotConnected") console.warn("snapshot:", erro);
  }
}

function digitando() {
  const ativo = document.activeElement;
  return ativo && (ativo.tagName === "INPUT" || ativo.tagName === "TEXTAREA");
}

// ------------------------------------------------------------ trocar de tela
//
// Toda transição desta janela é um `hidden` que sobe numa `<section class="tela">`
// e desce noutra. Isso desenha certo e deixa o teclado para trás.
//
// Esconder o ancestral do elemento focado devolve o foco ao `<body>`, e nada
// foca nada na tela que entra. As três consequências foram medidas com gente
// usando: quem abre a chamada pelo teclado cai no começo do documento e tabula
// até o botão de volta; quem volta não recebe o botão que apertou; e um leitor
// de tela não anuncia mudança nenhuma, porque do ponto de vista dele nada
// aconteceu. É WCAG 2.4.3 — a ordem do foco tem que preservar sentido.
//
// Três funções fecham isso, e as telas as chamam em volta do `hidden` que já
// escreviam. Nenhuma delas decide qual é a tela seguinte: isso continua sendo
// da tela que troca, e é a única coisa que ela sabe e este arquivo não.

/**
 * Onde o foco estava quando cada tela saiu de cena.
 *
 * Chaveado pela tela **deixada**, porque é isso que a volta pergunta: o que
 * estava focado da última vez que esta tela esteve na frente? Guarda o
 * elemento e não um `id` de propósito — metade dos controles desta janela é
 * desenhada pelo JavaScript e não tem `id` nenhum, e o botão de uma sala de voz
 * é reconstruído a cada snapshot.
 */
const focoDeVolta = new Map();

/**
 * Se dá para pôr o foco nisto agora.
 *
 * `focus()` num elemento escondido não faz nada e não avisa: o foco fica onde
 * estava, que depois de um `hidden` é o `<body>` — exatamente a falha que estas
 * funções existem para consertar, reintroduzida por dentro. Daí a conferência
 * antes, e não um `try`.
 *
 * `getClientRects()` vazio é uma resposta só para os três casos que a volta
 * encontra: escondido, arrancado da árvore no redesenho, e dentro de uma tela
 * que ainda não apareceu.
 */
function focavel(alvo) {
  return (
    Boolean(alvo) &&
    alvo.isConnected &&
    !alvo.disabled &&
    alvo.getClientRects().length > 0
  );
}

/**
 * Diz, para quem não vê, que a tela mudou.
 *
 * Um `role="status"` fora das telas, escrito **depois** de a troca acontecer.
 * Zerar antes não é higiene: uma região viva anuncia o que mudou nela, e
 * escrever a mesma frase por cima dela mesma não é mudança — sem isto, a
 * segunda visita seguida à mesma tela seria silenciosa, que é justo o caso de
 * quem abre e fecha a configuração para conferir uma coisa.
 */
function anunciar(frase) {
  const alvo = $("anuncio");
  alvo.textContent = "";
  // Num quadro à parte: zerar e escrever na mesma volta do laço de eventos é
  // uma mudança só, e o leitor de tela só vê o resultado dela.
  requestAnimationFrame(() => {
    alvo.textContent = frase;
  });
}

/**
 * Lembra o foco de uma tela, antes de escondê-la.
 *
 * Chamada com a tela ainda visível, ou não há foco nenhum para guardar.
 */
function guardarFoco(tela) {
  const raiz = $(tela);
  const ativo = document.activeElement;
  if (ativo && ativo !== document.body && raiz.contains(ativo)) {
    focoDeVolta.set(tela, ativo);
  } else {
    focoDeVolta.delete(tela);
  }
}

/**
 * Entra numa tela: põe o foco nela e anuncia a mudança.
 *
 * Não é o primeiro elemento. Cada tela nomeia o seu alvo em `data-foco`, ao
 * lado de si mesma na marcação, e escolhe o que ela existe para fazer — a
 * autenticação escolhe VERIFICAR IDENTIDADE, o fim escolhe SAIR, a chamada
 * escolhe a chave do microfone. Tabular até lá seria atravessar um cabeçalho
 * inteiro para chegar na única coisa que a tela pede.
 *
 * Sem `data-foco` o foco vai para a própria `<section>`, que carrega
 * `tabindex="-1"`. É o caso da operação, e é escolha e não omissão: aquela tela
 * não tem uma ação, tem quatro colunas — e o único controle plausível ali é um
 * campo de texto, que ligaria `digitando()` e desligaria o push-to-talk da
 * barra de espaço no instante em que a pessoa entra no servidor.
 *
 * Chamada **depois** de a tela desenhar, porque um botão ainda desabilitado
 * não aceita foco; `focavel` cobre o resto e cai na `<section>`.
 */
function abrirTela(tela, frase) {
  const raiz = $(tela);
  const preferido = raiz.dataset.foco ? $(raiz.dataset.foco) : null;
  (focavel(preferido) ? preferido : raiz).focus();
  anunciar(frase ?? raiz.dataset.anuncio ?? "");
}

/**
 * Volta para uma tela, devolvendo o foco a quem saiu dela.
 *
 * A outra metade de `guardarFoco`: quem apertou CHAMADA no cabeçalho recebe o
 * CHAMADA de volta ao fechar, e não o começo do documento. Sem o que guardar —
 * primeira visita, ou um controle que o redesenho arrancou — a tela é aberta
 * como qualquer outra.
 */
function voltarParaTela(tela) {
  const guardado = focoDeVolta.get(tela);
  focoDeVolta.delete(tela);
  if (!focavel(guardado)) {
    abrirTela(tela);
    return;
  }
  guardado.focus();
  anunciar($(tela).dataset.anuncio ?? "");
}

// ---------------------------------------------------- o que saiu daqui
//
// O modo `LEGENDAS SIMPLES` — `legendasSimples`, `aplicarLegendas`, a chave no
// `localStorage`, a classe `legendas-simples` no `body` e o interruptor no
// Terminal Dogma — não existe mais. Ele nasceu ligado por omissão e ninguém
// nunca o desligou, então o que ele de fato era é uma segunda forma de a mesma
// frase existir: escondida atrás de uma preferência que só quem construiu o app
// sabia que havia.
//
// O texto que aquele modo carregava **não** saiu junto. O que descreve a
// consequência de um controle é do produto e continua na tela, sempre visível,
// como `.nota` de `base.css`; o que descrevia o mecanismo por trás dele saiu com
// o modo. `apps/seele-app/tests/frontend.rs` prende as duas metades.
