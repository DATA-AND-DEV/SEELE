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

// ------------------------------------------------------------------------ MODs

/**
 * Carrega os MODs que o servidor desta janela exige.
 *
 * ADR 0045. Um MOD roda com acesso à janela inteira, por decisão: as regras do
 * produto base — palheta congelada, movimento diagnóstico, os quatro guardas do
 * vermelho — protegem o produto e **não** alcançam MOD. A defesa é o repositório
 * público obrigatório e a revisão de código de cada versão publicada, e ela não
 * mora aqui.
 *
 * Chamado no fim do arranque, depois de `ligarControlesDaJanela`, de propósito:
 * um MOD que rodasse antes de a página existir não acharia o que veio mexer, e
 * falharia de um jeito que parece defeito do MOD sem ser.
 */
const pedidosDeMod = new Map();
let proximoPedidoDeMod = 0;

/**
 * Quantos pedaços uma mídia do servidor pode pedir antes de o produto desistir.
 *
 * O teto por arquivo é um mega e cada resposta atravessa em partes de 10 KiB,
 * então cento e vinte e oito voltas cobrem o maior arquivo que o produto
 * aceita, com folga. Sem teto, um servidor que devolvesse `proximo` para sempre
 * faria a janela pedir para sempre — e o teto do arquivo não alcançaria, porque
 * ele só é conferido no fim.
 */
const PEDACOS_DE_MIDIA = 128;

/**
 * Escreve no registro de quem hospeda.
 *
 * **A janela não tinha como.** Todo o caminho de um pedido de MOD é observável
 * no Rust — o executor, a bomba, a ponte, o servidor —, e o pedaço que roda
 * aqui era um vão silencioso no meio dele. Um defeito que morasse nesse vão não
 * deixava rastro nenhum, e foi exatamente onde uma medição parou.
 *
 * Não vai para a tela: quem lê isto é quem hospeda, e quem usa não tem o que
 * fazer com a frase.
 */
function registrarNoAnfitriao(onde, o_que, nivel = "aviso") {
  // Nunca lança e nunca espera: um registro que derruba o caminho que ele
  // observa é pior que registro nenhum.
  try {
    invoke("registrar_da_janela", { nivel, onde, oQue: String(o_que) }).catch(() => {});
  } catch {
    /* sem ponte, sem registro — e o caminho segue */
  }
}

// ------------------------------------------------------- a geração da sessão
//
// **Qual execução de sessão é esta** — etapa E2 do contrato de API própria.
//
// O Rust é o dono do número: ele sobe a cada tentativa de conexão e a cada
// desmontagem, e nunca desce. A janela guarda uma cópia e a devolve em todo
// comando de MOD, e o Rust recusa a que não for a de pé.
//
// # Por que a janela precisa dela, se o Rust já confere
//
// Porque a maior parte do trabalho que sobra de uma sessão nunca chega ao Rust:
// é um `await` que volta, um worker que termina de subir, um desenho que ia
// para uma região. O número é o que permite a cada um desses perguntar «isto
// ainda é da minha sessão?» antes de tocar em qualquer coisa — que é o que o
// §5 do contrato chama de admitir efeito.
//
// Zero é «nenhuma sessão», e nada é admitido em nome dele.
let geracaoDaSessao = 0;

/** A janela soube de uma sessão nova. */
function entrarNaGeracao(numero) {
  geracaoDaSessao = Number(numero) || 0;
}

/** Esta geração ainda é a de pé? */
function daGeracaoDePe(numero) {
  return numero !== 0 && numero === geracaoDaSessao;
}

const ouvirMod = listen("seele://event", ({ payload }) => {
  const reply = payload?.ModReply;
  if (!reply) return;
  const pending = pedidosDeMod.get(reply.request);
  if (!pending || reply.total > 128 || reply.part >= reply.total) return;
  // **O número do pedido não basta para identificar o pedido.** Ele é um
  // contador desta janela, e ele não reinicia — mas a fila é esvaziada a cada
  // saída, e uma resposta atrasada da sessão anterior pode chegar depois de a
  // seguinte ter começado a numerar. Sem esta linha, ela seria entregue a quem
  // ocupou o número.
  if (!daGeracaoDePe(pending.geracao)) return;
  pending.parts[reply.part] = reply.payload;
  if (pending.parts.filter(v => v !== undefined).length === reply.total) {
    clearTimeout(pending.timer); pedidosDeMod.delete(reply.request);
    try { pending.resolve(JSON.parse(pending.parts.join(""))); }
    catch { pending.reject(new Error("invalid-response")); }
  }
});
/**
 * Leva um pedido de MOD ao servidor e devolve a resposta.
 *
 * **Deixou de ser um global** — ADR 0049. Enquanto o MOD rodava na página,
 * `globalThis.SeeleMods` era a API dele; congelar o objeto congelava aquele
 * objeto, e não os caminhos por onde se chega ao mesmo lugar. Agora o MOD está
 * num worker e não alcança nada daqui: ele **pede**, e quem decide o que
 * responder é esta função.
 *
 * O teto de oito pedidos em voo e o de 12 KiB continuam, e agora existem em
 * dois lugares de propósito: o prelúdio recusa cedo, para o MOD receber o erro
 * onde ele o escreveu, e aqui recusa de novo, porque um prelúdio é código que
 * roda dentro do worker e um worker é de quem escreveu o MOD.
 */
async function pedirAoServidor(id, canal, valor) {
  await ouvirMod;
  // A geração é lida **antes** do `await` do `invoke` e conferida depois: entre
  // as duas coisas a sessão pode ter acabado.
  const geracao = geracaoDaSessao;
  // **Cada recusa é dita antes de ser lançada.** Um `throw` daqui vira, lá
  // dentro, um `catch` do MOD ou uma promessa que ninguém pega — e em nenhum
  // dos dois casos quem hospeda fica sabendo qual das três portas fechou.
  if (!daGeracaoDePe(geracao)) {
    registrarNoAnfitriao("pedido-de-mod", `${id}: geração ${geracao} não é a de pé`);
    throw new Error("disconnected");
  }
  if (pedidosDeMod.size >= 8) {
    registrarNoAnfitriao("pedido-de-mod", `${id}: oito pedidos em voo, este não cabe`);
    throw new Error("too-many-requests");
  }
  const request = ++proximoPedidoDeMod;
  const payload = JSON.stringify(valor);
  if (new TextEncoder().encode(payload).length > 12 * 1024) {
    registrarNoAnfitriao("pedido-de-mod", `${id}: carga de ${payload.length} caracteres não cabe`);
    throw new Error("request-too-large");
  }
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pedidosDeMod.delete(request);
      registrarNoAnfitriao("pedido-de-mod", `${id}: pedido ${request} venceu o prazo sem resposta`);
      reject(new Error("timeout"));
    }, 15000);
    pedidosDeMod.set(request, { resolve, reject, timer, parts: [], geracao });
    invoke("mod_request", { geracao, request, id, channel: canal, payload }).catch(error => {
      clearTimeout(timer); pedidosDeMod.delete(request); reject(error);
    });
  });
}
/**
 * Põe um MOD de pé no executor do produto.
 *
 * O código vem pela ponte, e não por uma URL: a conferência de hash acontece
 * onde ela já mora — no Rust — e o que chega aqui é texto que já passou por ela.
 *
 * **Um executor, e só um.** O Worker de `blob:` que o ADR 0049 usou como
 * primeiro desenho foi reprovado por medição: ele herda a origem de quem o
 * criou, e com ela `indexedDB` e `caches` do produto — o que um MOD gravou lá
 * sobreviveu ao encerramento do aplicativo e reapareceu na entrada seguinte.
 * `terminate()` mata o contexto e não toca no armazenamento da origem.
 *
 * O QuickJS nativo não tem nada disso para herdar: o contexto não tem ambiente,
 * e a ausência não é uma jaula construída com cuidado — é o que um contexto de
 * QuickJS **é**. Ele deixou de ser configuração de bancada e passou a ser o
 * caminho normal; manter o outro como reserva seria manter, como reserva,
 * exatamente o que a medição reprovou.
 */
async function montarOMod(mod) {
  // **A geração é lida antes do `await` e conferida depois** — etapa E2.
  //
  // Antes, a conferência era `modsCarregados.has(mod.id)`, e o roteiro nomeia
  // por que ela não basta: «presença de um ID no Map não identifica uma
  // geração». Sair e entrar de novo no mesmo servidor repõe o mesmo
  // identificador no mesmo mapa — e a montagem atrasada da visita anterior
  // encontrava a chave dela lá, achava que ainda era a sua, e subia um worker
  // em cima da sessão nova.
  const geracao = geracaoDaSessao;
  const codigo = await invoke("codigo_do_mod", {
    geracao,
    id: mod.id,
    hash: mod.hash,
  });
  if (!daGeracaoDePe(geracao) || !modsCarregados.has(mod.id)) return;

  const executor = executorNativo(mod.id, geracao, mod.hash);

  // A instância é a dona de tudo o que este MOD criar — §4.2 do contrato. A
  // geração viaja com ela: quem atende as mensagens precisa saber de que sessão
  // ela é, e não de quem tem o identificador agora.
  const instancia = new InstanciaDeMod(mod.id, mod.hash, geracao, executor);

  // **Guardada e ativa antes de o executor subir**, e a ordem custou uma
  // medição para ser descoberta.
  //
  // O MOD começa a falar no instante em que o código dele roda. Guardando
  // depois, as primeiras mensagens chegavam a `atenderOMod` antes de a
  // instância estar no mapa — `meu()` respondia falso e elas eram descartadas
  // em silêncio. Com o Worker isso passava despercebido porque a ordem das
  // microtarefas escondia a corrida; com o executor nativo, que é outra
  // thread, o MOD ficava mudo e nada dizia por quê.
  //
  // Ativa antes de rodar não abre janela nenhuma: nada pode produzir efeito
  // antes de o executor começar, e `encerrar` alcança a instância de qualquer
  // estado.
  instancia.estado = ESTADOS_DE_MOD.ativa;
  modsCarregados.set(mod.id, instancia);

  await executor.iniciar(
    codigo,
    (mensagem) => atenderOMod(mod, instancia, mensagem),
    // **Um MOD que quebra não leva a janela junto** — ADR 0045, «falha
    // isolada». Fora do contexto da janela isso deixou de depender de cuidado.
    (erro) => {
      if (!instancia.admite(geracaoDaSessao)) return;
      console.error(`MOD ${mod.id}: erro`, erro);
      anotarEstadoDoMod(mod.id, "nao-carregou", erro ?? "");
    },
  );

  // E se a sessão acabou durante a subida, a instância sai — já guardada, já
  // alcançável, e por isso sem deixar runtime nenhum órfão.
  if (!daGeracaoDePe(geracao)) {
    modsCarregados.delete(mod.id);
    instancia.encerrar().catch(() => {});
    return;
  }
  // **`carregado` diz que os bytes executaram, e só isso.** Se o MOD estourou
  // dentro da própria inicialização, o worker subiu do mesmo jeito — quem sabe
  // disso é ele, e prometer o contrário seria inventar.
  anotarEstadoDoMod(mod.id, "carregado");
}

/**
 * A região de um MOD: onde ele desenha, e o único lugar onde ele desenha.
 *
 * ADR 0049. O MOD **declara** o que quer ver e o produto monta — com
 * `textContent` e `createElement`, nunca com HTML de texto: o conteúdo vem de
 * código de terceiro, e montá-lo como marcação seria dar a ele a página
 * inteira por outro caminho.
 *
 * A gramática é pequena de propósito. Cada forma nova é uma decisão de API, em
 * vez de um MOD descobrir que consegue.
 */
/**
 * As regiões de pé, por instância.
 *
 * **Por instância, e não por `id`.** Um MOD recarregado dentro da mesma sessão
 * é outra instância, e o que a região anterior segurava — mídia carregando,
 * ouvintes, quadros pendentes — não é dela. Guardar por `id` faria o
 * recarregamento herdar recursos de quem já saiu.
 */
const regioesDosMods = new Map();

/**
 * Desenha o que um MOD declarou, reaproveitando o que já está na tela.
 *
 * O trabalho mora em `RegiaoDeMod`, em `mods-regiao.js`. Esta função é a
 * amarração: onde a região vive, quem é o dono dela, e como ela fala de volta.
 */
function desenharARegiaoDoMod(mod, instancia, conteudo) {
  const palco = $("regioes-dos-mods");
  if (!palco) return;
  let regiao = regioesDosMods.get(instancia);
  if (!regiao) {
    const raiz = elemento("section", "regiao-de-mod");
    raiz.dataset.mod = mod.id;
    palco.append(raiz);
    regiao = new RegiaoDeMod(mod.id, donoDaRegiao(mod, instancia), raiz);
    regioesDosMods.set(instancia, regiao);
    // Registrada **na criação**, e não depois de o MOD desenhar: uma região
    // criada e não registrada é uma região que a saída não encontra.
    instancia.registrar(`${mod.id}: a região`, () => limparARegiaoDoMod(mod.id, instancia));
  }
  palco.hidden = false;
  const recusados = regiao.aplicar(conteudo);
  // **Recusar em silêncio é o defeito que este repositório mais paga.** Um MOD
  // cuja árvore não coube precisa saber disso onde ele a escreveu.
  if (recusados > 0) {
    throw new Error(`${recusados} nó(s) não couberam nos limites da região`);
  }
}

/**
 * Quem a região chama, e o que ela tem direito de fazer.
 *
 * As três perguntas do §5 do contrato numa função só: esta instância ainda é a
 * deste MOD, ela está ativa, e a geração dela é a de pé. Tudo o que a região
 * faz para fora — falar com o MOD, pedir um arquivo — passa por aqui.
 */
function donoDaRegiao(mod, instancia) {
  const meu = () =>
    modsCarregados.get(mod.id) === instancia && instancia.admite(geracaoDaSessao);
  return {
    instancia,
    geracao: instancia.geracao,
    podeFalar: meu,
    /**
     * Um evento da janela para o MOD.
     *
     * **Sem resposta e sem espera.** Um evento não é um pedido: quem aperta
     * uma tecla não fica esperando o MOD dizer que recebeu, e uma fila cheia
     * não pode travar a digitação. O que a fila recusar é contado e dito uma
     * vez, em vez de virar silêncio.
     */
    falar: (dados) => {
      if (!meu()) return;
      Promise.resolve(instancia.executor.entregar({ tipo: "evento", ...dados })).catch((falha) => {
        // Já saiu? Então a recusa é o desfecho certo, e não um defeito.
        if (!meu()) return;
        anotarEstadoDoMod(mod.id, "carregado", `evento recusado: ${falha?.message ?? falha}`);
      });
    },
    /**
     * Mídia que a **metade de servidor deste MOD** guarda.
     *
     * A da região com `fonte` vem do pacote; esta vem do servidor — a cena de
     * um tabuleiro, o retrato de um perfil. Quem pede é a janela, com o `id`
     * deste MOD, e os bytes vão direto do servidor para o Rust: uma imagem não
     * cabe numa declaração de região, e o MOD não precisa carregá-la na fila
     * dele para mostrá-la.
     */
    carregarMidiaDoServidor: async (canal, pedido, campo) => {
      const geracao = geracaoDaSessao;
      if (!meu()) throw new Error("disconnected");
      // **A continuação é opaca.** Uma imagem grande não cabe numa resposta só,
      // e o servidor do MOD diz como pedir o resto devolvendo um `proximo` que
      // o produto **não interpreta**: ele apenas o junta ao pedido seguinte.
      //
      // Interpretá-lo seria o produto conhecer a forma de um MOD — e a forma
      // muda por MOD. Esta foi escrita olhando dois deles pedirem paginação de
      // jeitos diferentes, e é a única que serve para os dois sem preferir um.
      let pedidoAtual = pedido;
      let base64 = "";
      for (let voltas = 0; voltas < PEDACOS_DE_MIDIA; voltas += 1) {
        const resposta = await pedirAoServidor(mod.id, canal, pedidoAtual);
        if (!daGeracaoDePe(geracao) || !meu()) throw new Error("disconnected");
        const pedaco = resposta?.[campo];
        if (typeof pedaco !== "string" || !pedaco) {
          // Dito pelo nome: um MOD cuja operação devolveu outra coisa precisa
          // saber **o quê**, e não «a mídia não carregou».
          throw new Error(`a resposta do servidor não traz «${campo}»`);
        }
        base64 += pedaco;
        if (!resposta.proximo || typeof resposta.proximo !== "object") break;
        pedidoAtual = { ...pedido, ...resposta.proximo };
      }
      const midia = await invoke("midia_em_bytes", { geracao, base64 });
      if (!daGeracaoDePe(geracao) || !meu()) throw new Error("disconnected");
      return midia;
    },
    /**
     * Abre o seletor do sistema e devolve o que a pessoa escolheu.
     *
     * O caminho no disco **não atravessa**: o que volta é um identificador, o
     * tipo que os bytes provaram ser e o tamanho. Os bytes ficam no Rust e
     * saem em pedaços por `pedacoDoArquivo`.
     */
    escolherArquivo: async () => {
      const geracao = geracaoDaSessao;
      if (!meu()) throw new Error("disconnected");
      const escolhido = await invoke("escolher_para_o_mod", { geracao, id: mod.id });
      // Depois do `await`: a pessoa pode ter demorado a escolher, e entregar
      // um arquivo a uma sessão que acabou é admitir efeito dela.
      if (!daGeracaoDePe(geracao) || !meu()) throw new Error("disconnected");
      return escolhido;
    },
    /** Um pedaço de um arquivo escolhido, em base64. */
    pedacoDoArquivo: async (arquivo, inicio) => {
      const geracao = geracaoDaSessao;
      if (!meu()) throw new Error("disconnected");
      const pedaco = await invoke("pedaco_do_escolhido", {
        geracao,
        id: mod.id,
        arquivo,
        inicio,
      });
      if (!daGeracaoDePe(geracao) || !meu()) throw new Error("disconnected");
      return pedaco;
    },
    /** Solta um arquivo escolhido, a pedido de quem o pediu. */
    soltarArquivo: (arquivo) => {
      const geracao = geracaoDaSessao;
      if (!meu()) return Promise.resolve();
      return invoke("soltar_escolhido", { geracao, id: mod.id, arquivo }).catch(() => {});
    },
    /** Um arquivo que o manifesto deste MOD declarou. */
    carregarMidia: async (caminho) => {
      const geracao = geracaoDaSessao;
      if (!meu()) throw new Error("disconnected");
      const midia = await invoke("midia_do_mod", {
        geracao,
        id: mod.id,
        hash: mod.hash,
        caminho,
      });
      // Depois do `await`: a sessão pode ter acabado enquanto os bytes vinham,
      // e montá-los seria tocar som de uma sessão que já não existe.
      if (!daGeracaoDePe(geracao) || !meu()) throw new Error("disconnected");
      return midia;
    },
  };
}

/** Tira a região de uma instância da tela, inteira — e o tema junto. */
function limparARegiaoDoMod(id, instancia) {
  const regiao = regioesDosMods.get(instancia);
  if (regiao) {
    regioesDosMods.delete(instancia);
    regiao.soltar();
  }
  const palco = $("regioes-dos-mods");
  // A faixa não existe vazia: uma régua de 1px em cima de nada é uma borda que
  // aparece sem ter o que separar.
  if (palco) palco.hidden = palco.childElementCount === 0;
  // O tema é por `id` de propósito: ele é sobre o nome que reservou o token, e
  // um MOD recarregado continua sendo o mesmo nome.
  if (temaDosMods.delete(id)) escreverOTemaDaSessao();
}

// ------------------------------------------------------------ o tema de um MOD

/**
 * O que cada MOD pediu, por `id`, na ordem em que pediu.
 *
 * Guardado em vez de escrito e esquecido porque a pergunta «este token já é de
 * alguém?» só tem resposta com isto aqui — e é ela que faz o segundo MOD ser
 * **recusado pelo nome** em vez de apagar o primeiro em silêncio.
 */
const temaDosMods = new Map();

/** Os quatro que a API conhece, e o nome do token que cada um redefine. */
const TEMA_DA_API = Object.freeze({
  fundo: "--seele-negro-absoluto",
  texto: "--seele-osso",
  acento: "--seele-laranja-nerv",
  borda: "--seele-linha",
  // **Painel e apagado entraram porque faltavam.** Um MOD de tema guardava os
  // seis no servidor e só conseguia aplicar quatro, e o que ele mostrava na
  // tela era um aviso de que os outros dois «aguardam suporte». Um aviso de
  // indisponibilidade no lugar de um comportamento é a forma mais barata de
  // não entregar uma coisa, e é por isso que ela não vale.
  painel: "--seele-negro-painel",
  apagado: "--seele-rotulo-painel",
});

/**
 * As medidas que um MOD pode escolher, e os valores que cada escolha vale.
 *
 * **Escolha, e não número.** Uma cor é um valor contínuo e o produto confere o
 * contraste dela; uma medida de espaçamento não tem como ser conferida assim —
 * um MOD que pedisse `0px` deixaria a sessão ilegível sem violar regra nenhuma.
 * Duas densidades, com os números do produto, é o que dá a escolha sem dar a
 * régua.
 */
const MEDIDAS_DA_API = Object.freeze({
  densidade: {
    compacta: { "--seele-celula-x": "6px", "--seele-celula-y": "10px" },
    confortavel: { "--seele-celula-x": "8px", "--seele-celula-y": "16px" },
  },
  // **A tipografia é recurso de sessão, e por isso ela cabe aqui.**
  //
  // O que a fazia ficar de fora era o medo certo: a escala de tipo do produto é
  // medida — tamanho, entrelinha e contraste andam juntos —, e um MOD que
  // escolhesse uma família qualquer moveria os três sem nada conferir o
  // resultado.
  //
  // Escolha resolve isso, como resolveu a densidade: o MOD nomeia, o produto
  // fornece a pilha, e as duas pilhas são as que o produto já usa e já mediu.
  // Escrita no contêiner da sessão como as cores, ela **sai com a sessão** e
  // não toca em preferência nenhuma de quem usa.
  fonte: {
    mono: { "--seele-mono": 'var(--seele-pilha-mono)' },
    sans: { "--seele-mono": 'var(--seele-pilha-sans)' },
  },
});

const COR_DO_TEMA = /^#[0-9a-f]{6}$/i;

/**
 * O tema que um MOD pede, aplicado **ao contêiner da sessão** — ADR 0049.
 *
 * O ESTILO escrevia os tokens em `document.documentElement`, e por isso a cor
 * dele ia para a tela de entrada, para o launcher e para a bateria. Não havia
 * onde escrevê-la que não fosse global, porque a sessão não tinha contêiner
 * próprio. Agora tem, e o tema é uma camada sobre ele: desmontar tira a camada
 * e a cor do produto reaparece, sem que ninguém fixe uma por cima.
 *
 * **Propriedade a propriedade, e não uma folha de estilo.** A CSP desta janela
 * é `style-src 'self'`, que recusa um `<style>` montado aqui dentro; a CSSOM
 * não passa por ela. O caminho pela CSP também seria o caminho por onde um
 * valor de terceiro entraria num arquivo de CSS, que é injeção — aqui ele
 * entra como valor de uma propriedade e nunca como texto de regra.
 *
 * Três recusas, todas pelo nome:
 *
 * - um valor que não é `#rrggbb`;
 * - um token que **outro MOD já pediu** — o produto não tem como saber qual dos
 *   dois quem usa quis, e escolher em silêncio é escolher errado metade das
 *   vezes;
 * - um par texto/fundo abaixo de 4,5:1. `tokens.css` mede e afirma o contraste
 *   de cada cor do produto; um tema que o derruba transforma aquelas linhas em
 *   promessa vencida, e quem paga é quem está lendo a conversa.
 */
function aplicarOTemaDoMod(id, valores) {
  const pedido = new Map();
  for (const [nome, valor] of Object.entries(valores ?? {})) {
    // **As medidas primeiro**, porque elas não são cor e a conferência de cor
    // recusaria o nome delas antes de alguém olhar.
    if (nome in MEDIDAS_DA_API) {
      if (!(valor in MEDIDAS_DA_API[nome])) {
        const conhecidas = Object.keys(MEDIDAS_DA_API[nome]).join(", ");
        throw new Error(`«${nome}» só aceita ${conhecidas}, e veio «${valor}»`);
      }
      pedido.set(nome, valor);
      continue;
    }
    if (!(nome in TEMA_DA_API)) {
      throw new Error(`a API de tema não conhece «${nome}»`);
    }
    if (typeof valor !== "string" || !COR_DO_TEMA.test(valor)) {
      throw new Error(`«${nome}» precisa ser uma cor #rrggbb, e veio «${valor}»`);
    }
    for (const [outro, seus] of temaDosMods) {
      if (outro !== id && nome in seus) {
        throw new Error(`«${nome}» já é do MOD «${outro}» nesta sessão`);
      }
    }
    pedido.set(nome, valor);
  }

  const antes = temaDosMods.get(id);
  temaDosMods.set(id, Object.fromEntries(pedido));
  const legivel = oTextoAlcancaOFundo();
  if (legivel !== true) {
    if (antes) temaDosMods.set(id, antes);
    else temaDosMods.delete(id);
    throw new Error(legivel);
  }
  escreverOTemaDaSessao();
}

/** Escreve no contêiner da sessão o que os MODs de pé pediram, e só isso. */
function escreverOTemaDaSessao() {
  const sessao = $("tela-sessao");
  if (!sessao) return;
  const emVigor = new Map();
  for (const seus of temaDosMods.values()) {
    for (const [nome, valor] of Object.entries(seus)) emVigor.set(nome, valor);
  }
  for (const [nome, token] of Object.entries(TEMA_DA_API)) {
    const valor = emVigor.get(nome);
    // Removida, e não reposta com a cor do produto: o recuo já está escrito em
    // `tela-sessao.css`, e repor aqui fixaria um valor por cima de quem, um dia,
    // escolher outro.
    if (valor) sessao.style.setProperty(token, valor);
    else sessao.style.removeProperty(token);
  }
  // As medidas saem pela mesma porta, e saem **juntas**: uma densidade é um
  // conjunto de números que só faz sentido inteiro.
  for (const [nome, escolhas] of Object.entries(MEDIDAS_DA_API)) {
    const escolhida = emVigor.get(nome);
    for (const [token, valor] of Object.entries(escolhas[escolhida] ?? {})) {
      sessao.style.setProperty(token, valor);
    }
    if (!escolhida) {
      for (const token of Object.keys(Object.values(escolhas)[0] ?? {})) {
        sessao.style.removeProperty(token);
      }
    }
  }
}

/**
 * O par texto/fundo que ficaria em vigor alcança 4,5:1?
 *
 * Devolve `true`, ou a frase da recusa. O que o MOD não pediu é lido **da
 * raiz** do documento, e não da sessão: a raiz é o único lugar onde a cor do
 * produto continua sendo a cor do produto com um tema de pé, e ler dali evita
 * uma segunda cópia da palheta aqui dentro, que envelheceria sozinha.
 */
function oTextoAlcancaOFundo() {
  const emVigor = {};
  for (const seus of temaDosMods.values()) Object.assign(emVigor, seus);
  if (!("texto" in emVigor) && !("fundo" in emVigor)) return true;

  const medido = getComputedStyle(document.documentElement);
  const daRaiz = (token) => medido.getPropertyValue(token).trim();
  const texto = emVigor.texto ?? daRaiz(TEMA_DA_API.texto);
  const fundo = emVigor.fundo ?? daRaiz(TEMA_DA_API.fundo);
  // Uma folha que não carregou não vira recusa: sem cor medida não há medida, e
  // inventar uma seria recusar um tema por um motivo que não aconteceu.
  if (!COR_DO_TEMA.test(texto) || !COR_DO_TEMA.test(fundo)) return true;

  const razao = contrasteEntre(texto, fundo);
  if (razao >= 4.5) return true;
  return `o texto «${texto}» sobre o fundo «${fundo}» dá ${razao.toFixed(2)}:1, `
    + "e a conversa precisa de 4,5:1 para ser lida";
}

/** WCAG 2.1: a razão de contraste entre duas cores `#rrggbb`. */
function contrasteEntre(a, b) {
  const luz = (cor) => {
    const canais = [1, 3, 5].map((i) => {
      const c = parseInt(cor.slice(i, i + 2), 16) / 255;
      return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * canais[0] + 0.7152 * canais[1] + 0.0722 * canais[2];
  };
  const [claro, escuro] = [luz(a), luz(b)].sort((x, y) => y - x);
  return (claro + 0.05) / (escuro + 0.05);
}

/** Responde a uma mensagem de um MOD, e só ao que a API dele oferece. */
async function atenderOMod(mod, instancia, m) {
  if (!m || typeof m.n !== "number") return;

  // **A conferência vem antes do efeito, e não só antes da resposta** — etapa
  // E2, e o roteiro aponta a linha: «checar a instância antes de `pedido`,
  // `snapshot`, `regiao` ou `tema`; hoje a checagem de worker atual está em
  // `responder`».
  //
  // A diferença é a que separa recusar de desfazer. Conferindo só na resposta,
  // uma mensagem que chegasse durante a saída **desenhava a região**, **pedia
  // ao servidor** ou **escrevia o tema** — e só então descobria que não tinha
  // com quem falar. O efeito já tinha acontecido; o que se economizava era o
  // `postMessage`.
  //
  // Duas perguntas, e as duas precisam ser feitas: este worker ainda é o deste
  // MOD (ele pode ter sido trocado por um recarregamento), e a sessão dele
  // ainda é a de pé (ela pode ter acabado).
  //
  // `admite` responde as duas de uma vez: a instância está em `ativa` — e não
  // em `encerrando`, que é onde ela fica entre o pedido de parada e a
  // confirmação — e a geração dela é a de pé.
  const minhaInstancia = () => modsCarregados.get(mod.id) === instancia;
  const meu = () => minhaInstancia() && instancia.admite(geracaoDaSessao);

  const responder = (ok, carga) => {
    // Perguntada **de novo** aqui, e não herdada de cima: entre a conferência
    // de entrada e esta linha há `await`s, e a sessão pode ter acabado no meio
    // deles. Mandar o **valor** para uma instância encerrada é falar com quem
    // já saiu.
    if (meu()) {
      instancia.executor.entregar({ tipo: "resposta", n: m.n, ok, ...carga });
      return;
    }
    // **Mas calar não é a alternativa.** Um pedido sem resposta deixa o MOD
    // esperando para sempre: `SeeleMods.request` é uma promessa, e ela não
    // rejeita sozinha. Medi isto no aplicativo nativo — o MOD parava na
    // segunda linha, vivo, mudo, sem nada no registro, e em três de seis
    // execuções. Recusar **não é admitir efeito**: é o contrário dele, e é a
    // única coisa que devolve o controle a quem escreveu o MOD.
    //
    // Só para a instância que ainda é a deste MOD: se outra já tomou o lugar,
    // esta não tem a quem responder.
    if (minhaInstancia()) {
      instancia.executor.entregar({
        tipo: "resposta",
        n: m.n,
        ok: false,
        erro: "sessao-encerrada",
      });
    }
  };
  if (!minhaInstancia()) {
    // **O último caminho calado, e ele é dito.** Aqui não há a quem responder:
    // outra instância já tomou o lugar desta, e falar com esta seria falar com
    // quem saiu. Mas o MOD que perguntou fica esperando para sempre, e quem
    // hospeda tem direito de saber que isso aconteceu — era o único vão do
    // caminho de um pedido que ainda não deixava rastro.
    registrarNoAnfitriao(
      "atender-mod",
      `${mod.id}: «${m.tipo}» chegou de uma instância que já não é a carregada`,
    );
    return;
  }
  if (!meu()) {
    responder(false, { erro: "sessao-encerrada" });
    return;
  }
  try {
    switch (m.tipo) {
      case "pedido":
        responder(true, { valor: await pedirAoServidor(mod.id, m.canal, m.valor) });
        break;
      case "snapshot": {
        const valor = await invoke("snapshot");
        // Depois do `await`: a sessão pode ter acabado enquanto o retrato
        // vinha, e entregá-lo daria ao MOD o estado de uma sessão que já não é
        // a dele. `responder` recusa nesse caso — **e recusa é obrigatório**:
        // era este `return` calado que prendia o MOD para sempre.
        responder(true, { valor });
        break;
      }
      case "regiao":
        desenharARegiaoDoMod(mod, instancia, m.conteudo);
        responder(true, { valor: null });
        break;
      case "tema":
        aplicarOTemaDoMod(mod.id, m.valores);
        responder(true, { valor: null });
        break;
      case "pedaco": {
        const pedaco = await donoDaRegiao(mod, instancia).pedacoDoArquivo(m.arquivo, m.inicio);
        if (!meu()) {
          responder(false, { erro: "sessao-encerrada" });
          break;
        }
        responder(true, { valor: pedaco });
        break;
      }
      case "soltar-arquivo":
        await donoDaRegiao(mod, instancia).soltarArquivo(m.arquivo);
        responder(true, { valor: null });
        break;
      default:
        // **Recusado e nomeado.** Uma mensagem que a API não conhece não pode
        // ser ignorada: quem escreveu o MOD ficaria esperando para sempre uma
        // resposta que nunca vem, sem saber por quê.
        responder(false, { erro: `a API de MODs não conhece «${m.tipo}»` });
    }
  } catch (falha) {
    const motivo = String(falha?.message ?? falha);
    registrarNoAnfitriao("atender-mod", `${mod.id}: «${m.tipo}» falhou — ${motivo}`);
    responder(false, { erro: motivo });
  }
}

const modsCarregados = new Map();
let conferindoMods = false;
let ultimoErroDeMods = "";

// **O que aconteceu com cada MOD, por fase** — A06 da auditoria de 17/09.
//
// Tudo aqui dentro comunicava por `console.warn` e `console.error`: pacote
// ausente, hash divergente, catálogo recusado e script que não carregou. A
// gestão mostrava instalado e ligado, e nunca «carregado», «falhou» ou
// «incompatível». Quem usa ficava sem próximo passo, e num aplicativo
// empacotado o console não é lugar nenhum.
//
// A distinção que a auditoria pede está nos nomes: `carregado` quer dizer que
// os **bytes executaram**, e não que o MOD terminou de se inicializar — isso o
// MOD sabe e nós não, e prometer o segundo seria inventar.
const estadoDosMods = new Map();

/**
 * O que **este** servidor exige agora, por `id`.
 *
 * Separado de `estadoDosMods` porque responde outra pergunta. A fase diz o que
 * aconteceu com um MOD — carregou, faltou o pacote, veio outra versão. Esta
 * lista diz se ele é **obrigatório aqui**, que é o que a gestão precisa saber
 * para não oferecer a quem entrou um interruptor de desligar o que o servidor
 * exige: desligá-lo seria sair do conjunto acordado sem sair do servidor.
 *
 * Vazia fora de sessão, e por isso não guarda o que o último servidor exigia:
 * a exigência é de um destino, e não desta máquina.
 *
 * **Guarda o hash, e não só o identificador.** A gestão desenha uma linha para
 * o que o servidor exige e não está aqui — e sem o hash aquela linha diria «o
 * servidor exige este MOD» sem dizer *qual conteúdo*, que é a única coisa que
 * quem for instalá-lo precisa conferir.
 */
const modsExigidos = new Map();

/**
 * A identidade do conjunto que esta janela já tentou completar.
 *
 * **Uma tentativa por conjunto, e não uma por tique.** `carregarMods` roda a
 * cada quatro segundos; sem isto, um catálogo fora do ar viraria uma busca a
 * cada quatro segundos, para sempre, contra um servidor de outra pessoa.
 *
 * Zerada ao encerrar a sessão: entrar de novo é uma decisão nova de quem usa, e
 * é o gesto que pede «tente outra vez».
 */
let conjuntoJaBuscado = "";

/**
 * Põe de volta no disco o que **este mesmo conjunto** já autorizou.
 *
 * # Por que a conferência de identidade é a regra inteira
 *
 * O aceite autoriza um conjunto exato — o que estava escrito na tela quando a
 * pessoa disse sim. Buscar qualquer coisa que o servidor anuncie depois seria
 * ampliar aquele consentimento para uma lista que ninguém leu.
 *
 * Então só há busca quando a identidade anunciada agora é **igual** à que está
 * gravada para este destino. Diferente, não se busca nada: uma lista nova faz a
 * entrada falhar com a pergunta de sempre, que é onde ela pertence.
 */
async function buscarOQueFaltaSeJaFoiAceito(catalogo, ausentes) {
  // `alvoDaSessao` mora em `tela-sessao.js`, que carrega depois deste arquivo.
  // Na primeira chamada — a que roda no fim de `base.js` — ela ainda não
  // existe, e não existir aqui quer dizer «não há sessão», que é a resposta
  // certa de qualquer forma.
  const alvo = typeof alvoDaSessao === "function" ? alvoDaSessao() : null;
  if (!alvo || !catalogo.conjunto || conjuntoJaBuscado === catalogo.conjunto) return;

  let aceito = null;
  try {
    aceito = await invoke("aceite_de_mods", { alvo });
  } catch (falha) {
    console.warn("aceite deste destino:", falha);
    return;
  }
  if (aceito !== catalogo.conjunto) return;

  conjuntoJaBuscado = catalogo.conjunto;
  for (const m of ausentes) {
    anotarEstadoDoMod(m.id, "obtendo");
    try {
      await invoke("instalar_mod_do_catalogo", { id: m.id, versao: m.version });
    } catch (falha) {
      // **Visível, e não só no console.** A falha é de um MOD que o servidor
      // exige: sem frase, a sessão continuaria sem ele e a pessoa descobriria
      // pela cor que não mudou.
      anotarEstadoDoMod(m.id, "nao-obtive", String(falha?.message ?? falha));
    }
  }
}

/** Anota a fase de um MOD e avisa quem desenha. */
function anotarEstadoDoMod(id, fase, detalhe = "") {
  const antes = estadoDosMods.get(id);
  if (antes?.fase === fase && antes?.detalhe === detalhe) return;
  estadoDosMods.set(id, { fase, detalhe });
  globalThis.dispatchEvent(new CustomEvent("seele-mods-estado"));
}
async function carregarMods() {
  if (conferindoMods) return;
  conferindoMods = true;
  let instalados;
  try {
    await invoke("snapshot");
    instalados = await invoke("mods_instalados");
    // Pelo caminho de dentro, e não pela API dos MODs: o `id` vazio é o
    // catálogo do próprio servidor, e não um MOD pedindo alguma coisa. Enquanto
    // isto passava por `globalThis.SeeleMods`, o produto dependia do objeto que
    // ele expunha aos MODs — e o ADR 0049 o tirou da janela.
    const catalogo = await pedirAoServidor("", 0, {});
    if (!catalogo.ok) throw new Error("catalogue-refused");
    // O catálogo respondeu: a notícia de que ele não respondia deixou de valer.
    if (estadoDosMods.has("")) {
      estadoDosMods.delete("");
      globalThis.dispatchEvent(new CustomEvent("seele-mods-estado"));
    }
    for (const [id, instancia] of modsCarregados) {
      if (!catalogo.mods.some(m => m.id === id)) {
        // Pelo ciclo único: `encerrar` põe a instância em `encerrando` antes de
        // qualquer espera, pede ao executor que pare, espera ele confirmar, e
        // só então descarta os recursos dela — a região entre eles.
        instancia?.encerrar().catch(() => {});
        modsCarregados.delete(id);
        anotarEstadoDoMod(id, "descarregado");
      }
    }
    // E o que voltou a bater deixa de ser notícia ruim: sem isto, um MOD que
    // aparecia como «outra versão» continuaria assim depois de instalado.
    for (const m of catalogo.mods) {
      if (["sem-pacote", "outra-versao"].includes(estadoDosMods.get(m.id)?.fase)
          && instalados.some(i => i.id === m.id && i.hash === m.hash)) {
        estadoDosMods.delete(m.id);
        globalThis.dispatchEvent(new CustomEvent("seele-mods-estado"));
      }
    }
    // O conjunto exigido por este destino, como ele acabou de ser anunciado.
    // Reescrito por inteiro a cada volta: um MOD que saiu da exigência tem de
    // sair daqui junto, senão a gestão continua chamando de obrigatório o que
    // o servidor já soltou.
    modsExigidos.clear();
    for (const m of catalogo.mods) modsExigidos.set(m.id, m.hash);

    const ausentes = catalogo.mods.filter(active => !instalados.some(m => m.id === active.id && m.hash === active.hash));
    for (const m of ausentes) {
      // **Duas causas, duas frases.** Ter o id e não ter o hash é versão
      // diferente da exigida — e mandar «instale» quem já instalou é mandar
      // fazer de novo o que não resolve.
      const temOId = instalados.some(i => i.id === m.id);
      // O conteúdo exigido vai nas **duas** frases. Sem ele, «não está
      // instalado aqui» manda instalar sem dizer o quê: o catálogo pode ter
      // mais de uma versão, e instalar a errada dá a mesma tela de novo.
      anotarEstadoDoMod(
        m.id,
        temOId ? "outra-versao" : "sem-pacote",
        `o servidor exige o conteúdo ${m.hash.slice(0, 16)}…`,
      );
    }
    // **O que já foi aceito e sumiu do disco é buscado de volta.**
    //
    // Matriz de aceite do plano de 18/09: «aceite salvo e cache removido → o
    // fluxo obtém o hash exato antes de ficar pronto; falhas são visíveis».
    //
    // Até aqui isto valia só no caminho que passa pela tela de aceite. Quem já
    // tinha dito sim entrava direto, e se o pacote não estivesse mais no disco
    // — apagado pela manutenção local, ou numa máquina que trocou — a sessão
    // começava sem ele e ninguém dizia nada.
    await buscarOQueFaltaSeJaFoiAceito(catalogo, ausentes);

    const diagnostico = ausentes.map(m => m.id).join(", ");
    ultimoErroDeMods = diagnostico;
    instalados = instalados.filter(m => catalogo.mods.some(active => active.id === m.id && active.hash === m.hash));
  } catch (erro) {
    const mensagem = String(erro);
    // Sem sessão não há exigência nenhuma de pé. Deixar a lista cheia faria a
    // gestão dizer «exigido por este servidor» depois de sair dele.
    if (modsExigidos.size > 0) {
      modsExigidos.clear();
      globalThis.dispatchEvent(new CustomEvent("seele-mods-estado"));
    }
    if (!mensagem.includes("NotConnected") && mensagem !== ultimoErroDeMods) {
      console.warn("Não foi possível conferir os MODs:", erro);
      ultimoErroDeMods = mensagem;
      // Sem sessão não há o que dizer: a lista de exigidos é do servidor, e
      // ninguém prometeu nada a conferir. Com sessão, a falha é notícia.
      anotarEstadoDoMod("", "catalogo-recusado", mensagem);
    }
    conferindoMods = false;
    return;
  }

  for (const mod of instalados) {
    if (!mod.client || modsCarregados.has(mod.id)) continue;
    // Marcado **antes** do `await`: o laço roda a cada quatro segundos, e sem
    // isto dois tiques sobrepostos criariam dois workers do mesmo MOD.
    modsCarregados.set(mod.id, null);
    anotarEstadoDoMod(mod.id, "carregando");
    montarOMod(mod).catch((falha) => {
      console.error(`MOD ${mod.id}: não carregou`, falha);
      anotarEstadoDoMod(mod.id, "nao-carregou");
      modsCarregados.delete(mod.id);
    });
  }
  conferindoMods = false;
}

ligarControlesDaJanela();
carregarMods();
setInterval(carregarMods, 4000);
/**
 * Encerra o ambiente dos MODs desta sessão. **Um caminho só, e idempotente.**
 *
 * Antes isto morava dentro do ouvinte de `Ended`, e `Ended` só chega do outro
 * lado: a sala terminou, o enlace caiu, alguém foi moderado. **Sair por
 * vontade própria não emite `Ended`** — `Connection::disconnect` manda
 * `Shutdown`, e o laço responde `return false` sem avisar ninguém.
 *
 * O efeito era o que se via: o ESTILO escreve os tokens de tema em
 * `document.documentElement`, e `restore` só roda no descarte cooperativo. Quem
 * saía do servidor levava a cor dele para a entrada e para o launcher, e a
 * única forma de tirá-la era fechar o app.
 *
 * Chamado de todo caminho que sai: saída local, término remoto, queda,
 * expulsão. Chamar duas vezes é seguro e é o ponto — quem sai não precisa saber
 * se o outro lado também vai avisar.
 */
function encerrarOAmbienteDosMods() {
  // **Revogar primeiro** — §5 do contrato, passo 1. A janela deixa de admitir
  // efeito em nome desta sessão **antes** de começar a desmontar: entre a
  // primeira linha e a última há mensagens de worker em voo, `await`s voltando
  // e um laço de quatro segundos que pode acordar no meio. Zerar aqui fecha
  // todos de uma vez, porque `daGeracaoDePe` passa a responder falso para tudo
  // o que a sessão anterior deixou pelo caminho.
  //
  // Idempotente: chamar de novo com zero não faz nada, e é o desfecho certo —
  // quem sai não precisa saber se o outro lado também vai avisar.
  geracaoDaSessao = 0;

  for (const [, instancia] of modsCarregados) {
    // **Parar é do produto, e não um pedido ao MOD** — ADR 0049.
    //
    // Antes isto disparava `seele-mod-unload` e o MOD desmontava a si mesmo. Um
    // MOD que não implementasse o evento deixava temporizador, ouvinte, regra
    // de CSS e áudio de pé — e isso não era defeito dele: era o que «rodar na
    // página» significava.
    //
    // O ciclo é um só e é o da instância: `encerrando` antes de qualquer
    // espera, o executor parando, a confirmação, e então os recursos. **A
    // promessa não é esperada aqui**, e é deliberado: sair da sessão não pode
    // ficar preso num executor que demora a confirmar. Quem precisa saber se
    // acabou de verdade olha o estado da instância, que é o que a bancada faz.
    instancia?.encerrar().catch(() => {});
  }
  modsCarregados.clear();
  // A exigência é de um destino, e o destino acabou. Esperar o tique seguinte
  // deixaria a gestão dizendo «exigido por este servidor» por até quatro
  // segundos depois de a sessão ter terminado.
  modsExigidos.clear();
  // Entrar de novo é uma decisão nova, e é o gesto que pede «tente outra vez»:
  // uma busca que falhou não fica falhada para sempre.
  conjuntoJaBuscado = "";
  globalThis.dispatchEvent(new CustomEvent("seele-mods-estado"));
  // Passo 2 do §5: cancelar o que estava em curso e recusar o que chegar tarde.
  // O `clear` é o que garante o segundo — uma resposta que chegue depois não
  // acha mais o pedido, e a conferência de geração no ouvinte a recusaria de
  // qualquer forma.
  for (const p of pedidosDeMod.values()) { clearTimeout(p.timer); p.reject(new Error("disconnected")); }
  pedidosDeMod.clear();
}

listen("seele://event", ({ payload }) => {
  if (!payload?.Ended) return;
  encerrarOAmbienteDosMods();
});
