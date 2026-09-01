// SEELE — a camada comum a toda tela.
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
 * `specs/06-clientes-gui.md`: nenhuma informação transmitida só por cor. A marca
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
// Terminal servidor — não existe mais. Ele nasceu ligado por omissão e ninguém
// nunca o desligou, então o que ele de fato era é uma segunda forma de a mesma
// frase existir: escondida atrás de uma preferência que só quem construiu o app
// sabia que havia.
//
// O texto que aquele modo carregava **não** saiu junto. O que descreve a
// consequência de um controle é do produto e continua na tela, sempre visível,
// como `.nota` de `base.css`; o que descrevia o mecanismo por trás dele saiu com
// o modo. `apps/seele-app/tests/frontend.rs` prende as duas metades.

/**
 * Fecha uma camada quando se aperta fora da caixa dela.
 *
 * O `Escape` sozinho não bastava, e a razão é de quem usa: ele está longe da
 * mão que acabou de clicar, e quem nunca leu a documentação não tem por que
 * saber que ele fecha. Apertar fora é o gesto que todo mundo já tenta primeiro.
 *
 * # Por que aqui e não quatro vezes
 *
 * São quatro camadas — ajuda, compartilhar, moderação e portaria — e cada uma
 * já repetia o próprio `Escape`. Uma quinta camada escrita amanhã ganha isto de
 * graça ao chamar esta função, e não por lembrar de copiar um ouvinte.
 *
 * # O alvo é o véu, e só ele
 *
 * `evento.target === camada` e não `!caixa.contains(...)`: um clique que começa
 * dentro da caixa e termina fora — arrastar para selecionar um texto e soltar
 * no escuro — dispara no elemento onde **começou**, e fechar ali apagaria da
 * tela o que a pessoa estava lendo. O `mousedown` é conferido junto pelo mesmo
 * motivo: sem ele, soltar o botão fora depois de arrastar de dentro fecharia.
 */
function fecharAoClicarFora(id, fechar) {
  const camada = $(id);
  if (!camada) return;
  let comecouNoVeu = false;
  camada.addEventListener("mousedown", (evento) => {
    comecouNoVeu = evento.target === camada;
  });
  camada.addEventListener("click", (evento) => {
    if (comecouNoVeu && evento.target === camada) fechar();
    comecouNoVeu = false;
  });
}

// --------------------------------------------------- recarregar não é opção
//
// **Recarregar quebra o produto, e o menu do botão direito oferece isso.**
//
// A janela é uma casca sobre uma sessão que vive no Rust. Um `location.reload()`
// não derruba a sessão — ela continua conectada, o áudio continua correndo —,
// mas joga fora tudo que esta camada sabe: em que tela se estava, qual Linha
// estava aberta, o histórico desenhado. O resultado é a tela de entrada por
// cima de uma conversa em andamento, e nada dizendo o que aconteceu.
//
// Num site, recarregar é o gesto universal de «tenta de novo». Aqui é o gesto
// que estraga. Nenhum aviso resolve isso: a pessoa que aperta recarregar já
// decidiu que é inofensivo, porque em todo lugar é.
//
// ---- por que não simplesmente apagar o menu ----
//
// Porque o mesmo menu carrega copiar e colar, e num campo de texto eles são
// úteis de verdade. Apagar tudo trocaria um defeito por outro, e o segundo
// atingiria quem só queria colar um endereço.
//
// Então o menu **fica onde edita e onde há texto escolhido**, e some no resto —
// que é exatamente onde o item «recarregar» mora.
//
// ---- e o teclado ----
//
// F5, Ctrl+R e ⌘R fazem a mesma coisa sem passar por menu nenhum, e são o
// caminho de quem tem o dedo treinado. Ficam bloqueados pelo mesmo motivo.

/** Se este alvo é um lugar onde o menu do sistema serve para alguma coisa. */
function editavel(alvo) {
  if (!alvo || !alvo.closest) return false;
  return Boolean(alvo.closest("input, textarea, [contenteditable='true']"));
}

window.addEventListener("contextmenu", (evento) => {
  if (editavel(evento.target)) return;
  // Texto escolhido com o mouse: o menu é como se copia sem saber o atalho.
  const escolha = window.getSelection();
  if (escolha && !escolha.isCollapsed && String(escolha).trim() !== "") return;
  evento.preventDefault();
});

window.addEventListener(
  "keydown",
  (evento) => {
    const recarrega =
      evento.key === "F5" ||
      ((evento.ctrlKey || evento.metaKey) && (evento.key === "r" || evento.key === "R"));
    if (recarrega) evento.preventDefault();
  },
  // Fase de captura, para chegar antes de qualquer tela que também escute
  // teclado. Um `preventDefault` tardio não impede o navegador de recarregar.
  true,
);

// ------------------------------------------------- a barra da janela
//
// Ela substitui a barra de título do sistema (comp da 0.9.0). O que ela faz
// aqui é pouco e é tudo: descobrir em que sistema está, mover os controles de
// acordo, e ligar os três botões à janela.
//
// **Por que a plataforma vem do Rust e não do `navigator`.** Porque o que se
// quer saber não é qual motor desenha, é qual convenção de janela vale — e o
// `userAgent` de um webview responde a primeira. O comando `plataforma` é o
// mesmo `cfg` que decidiu tirar a decoração, então os dois não podem discordar.

/** Põe na barra a plataforma, que é o que o CSS lê para arrumá-la. */
async function arrumarBarraDaJanela() {
  try {
    const onde = await invoke("plataforma");
    $("barra-janela").dataset.plataforma = onde;
  } catch (falha) {
    // Sem resposta, a barra fica na forma do Windows — os três controles
    // desenhados. É o lado seguro: num Mac eles aparecem duplicados e feios;
    // no Windows, a ausência deles é uma janela que não fecha.
    console.warn("plataforma:", falha);
    $("barra-janela").dataset.plataforma = "windows";
  }
}

arrumarBarraDaJanela();

// O relógio da barra. Local e não do servidor — é a hora de quem está olhando.
setInterval(() => {
  $("barra-relogio").textContent = new Date().toLocaleTimeString();
}, 1000);

/**
 * Liga os três controles à janela desta casca.
 *
 * `getCurrentWindow` do Tauri, e não um comando nosso: minimizar, maximizar e
 * fechar são da janela e não do produto, e escrever três comandos no `main.rs`
 * seria três lugares nossos para um verbo que já existe pronto.
 */
function ligarControlesDaJanela() {
  const janela = window.__TAURI__?.window?.getCurrentWindow?.();
  if (!janela) {
    // Fora do Tauri — um navegador aberto no `index.html` para olhar o
    // desenho. Os botões ficam ali sem fazer nada, que é melhor que rebentar
    // o resto do arquivo.
    console.warn("sem janela do Tauri; os controles da barra não farão nada");
    return;
  }
  $("janela-minimizar").addEventListener("click", () => {
    janela.minimize().catch((falha) => console.warn("minimizar:", falha));
  });
  $("janela-maximizar").addEventListener("click", () => {
    janela.toggleMaximize().catch((falha) => console.warn("maximizar:", falha));
  });
  $("janela-fechar").addEventListener("click", () => {
    janela.close().catch((falha) => console.warn("fechar:", falha));
  });
}

ligarControlesDaJanela();
