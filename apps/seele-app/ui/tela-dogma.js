// SEELE · Entry Plug — o Terminal Dogma (`#tela-dogma`).
//
// A configuração local: o que é desta máquina e não deste Dogma. Quatro seções
// — ÁUDIO, ATALHOS, APARÊNCIA, IDENTIDADE —, que é a forma do comp v3, §8.
//
// Alcançável das duas telas vivas, e volta para a que a abriu. Escolher
// microfone antes de conectar é tão comum quanto durante, e uma configuração
// atrás da sessão poria o controle atrás da porta que ele serve para abrir.
//
// ---- não há SALVAR, e a ausência é a decisão ----
//
// O comp desenha `SALVAR`, `DESCARTAR` e um estado sujo. Nada disso está aqui.
// A escolha vale na hora, e o que a tela mostra é o que **está valendo** — a
// lista de microfones diz qual abriu, e não qual foi pedido. Confirmação por
// estado, não por botão: um `SALVAR` num painel de áudio promete que nada muda
// até apertá-lo, e isso é falso para som, porque você precisa ouvir o efeito
// para saber se escolheu certo.
//
// ---- o que este arquivo não decide ----
//
// Nada. A lista de dispositivos vem do Rust, a escolha vai para o Rust, e é lá
// que ela é gravada e aplicada. Este arquivo não sabe o que é um id de
// dispositivo: ele o desenha nunca e o devolve inteiro. O modo de voz é o mesmo
// acordo — os três nomes chegam e voltam como chegaram, e nada aqui sabe o que
// cada um faz com o microfone.

"use strict";

/** Para onde VOLTAR devolve. `null` enquanto esta tela está fechada. */
let telaDeOrigem = null;

/**
 * O id escolhido, ou `null` para o padrão da máquina.
 *
 * Guardado aqui só para desenhar qual linha está acesa entre uma leitura e a
 * seguinte. A verdade mora no disco, do lado do Rust; isto é uma cópia da
 * última resposta dele, e toda escolha o consulta de novo.
 */
let microfoneEscolhido = null;

/** Quantos blocos o medidor de entrada tem. 26, como no comp. */
const BLOCOS_DO_MEDIDOR = 26;

/**
 * O que a porta de saída diz, conforme de onde se entrou.
 *
 * O comp escreve `VOLTAR AO DOGMA` e daqui isso só é verdade metade das vezes:
 * esta tela também abre da entrada, onde não há Dogma nenhum para voltar. Um
 * botão que promete o lugar errado é pior que um botão genérico.
 */
const VOLTA = {
  "tela-boot": "VOLTAR À ENTRADA",
  "tela-sessao": "VOLTAR AO DOGMA",
};

// ------------------------------------------------------------------- seções

/**
 * Abre uma seção e fecha as outras.
 *
 * O título e o subtítulo saem do `data-titulo` e do `data-sub` do próprio
 * botão. Uma tabela em JavaScript com as mesmas oito frases seria uma segunda
 * lista para alguém esquecer de atualizar no dia em que uma seção mudar de
 * nome — e o comp já erra assim, com `SECOES` longe da marcação que a desenha.
 *
 * `aria-current` e não só a barra laranja: a barra é o que sobra em
 * monocromático, e o `aria-current` é o que atravessa para quem não vê nenhuma
 * das duas.
 */
function abrirSecao(id) {
  for (const botao of document.querySelectorAll(".dogma-secao")) {
    const atual = botao.id === id;
    botao.setAttribute("aria-current", atual ? "true" : "false");
    $(botao.dataset.painel).hidden = !atual;
    if (!atual) continue;
    $("dogma-titulo").textContent = botao.dataset.titulo;
    $("dogma-subtitulo").textContent = botao.dataset.sub;
  }
}

// ------------------------------------------------------------------- desenho

/**
 * A lista de microfones, com o escolhido aceso.
 *
 * A linha de cima é sempre o padrão da máquina. Não é um dispositivo: é a
 * ausência de escolha, e ela precisa ser escolhível de volta — sem ela, quem
 * experimentou uma interface e a desconectou ficaria com uma preferência
 * apontando para um aparelho que não existe mais e sem nada na tela que a
 * desfaça.
 */
async function desenharMicrofones() {
  const [dispositivos, escolhido] = await Promise.all([
    invoke("microfones"),
    invoke("microfone_escolhido"),
  ]);
  microfoneEscolhido = escolhido ?? null;

  const lista = $("lista-microfones");
  if (dispositivos.length === 0) {
    // Lista vazia é "a máquina não quis enumerar", e não "não há microfone".
    // Quem escreve a segunda frase aqui mente para quem tem áudio funcionando.
    const vazio = elemento("li", "dogma-dispositivos-vazio", "ESTA MÁQUINA NÃO LISTOU DISPOSITIVO NENHUM");
    repovoar(lista, [vazio]);
    return;
  }

  const linhas = [linhaDeMicrofone("", "PADRÃO DA MÁQUINA", false)];
  for (const dispositivo of dispositivos) {
    linhas.push(linhaDeMicrofone(dispositivo.id, dispositivo.name, dispositivo.default));
  }
  repovoar(lista, linhas);
  // Marcadas na mesma tarefa em que nascem. Deixar para o snapshot seguinte
  // deixaria um quadro com a lista inteira apagada, e o quadro apagado é o que
  // diz "nenhum destes está escolhido".
  marcarLinhas(null);
}

/**
 * Uma linha da lista. `id` vazio é o padrão da máquina, que não é dispositivo
 * nenhum: é a ausência de escolha, e precisa ser escolhível de volta.
 *
 * ---- a lista de saídas, quando ela vier ----
 *
 * `#lista-saidas` está desenhada com a mesma forma e, hoje, com uma linha só:
 * `seele-audio/src/device.rs` abre a saída padrão e ainda não sabe enumerar as
 * outras. Quando a enumeração chegar, o preenchimento é este mesmo, com os três
 * comandos irmãos dos de captura — `saidas`, `saida_escolhida` e
 * `escolher_saida` —, e a linha desabilitada de `index.html` sai no lugar.
 */
function linhaDeMicrofone(id, nome, ehPadrao) {
  const linha = elemento("li");
  const botao = elemento("button", "dogma-dispositivo");
  botao.type = "button";
  botao.dataset.dispositivo = id;
  botao.dataset.padrao = ehPadrao ? "sim" : "nao";
  botao.append(
    elemento("span", "dogma-dispositivo-nome", nome),
    elemento("span", "dogma-dispositivo-marca"),
  );
  // O id sai daqui exatamente como entrou. Nada nesta janela o interpreta —
  // vazio vira `null`, que é como o Rust escreve "o padrão da máquina".
  botao.addEventListener("click", () => escolher(id === "" ? null : id));

  linha.append(botao);
  return linha;
}

/**
 * Acende o que está escolhido e diz o que está aberto.
 *
 * São duas coisas e não uma, e é a divergência entre elas que esta tela existe
 * para mostrar: o escolhido é o que ficou gravado, e o aberto é o que a máquina
 * conseguiu abrir. Empatam na maior parte do tempo e separam justamente quando
 * importa — a interface escolhida ontem que está desconectada hoje aparece como
 * ESCOLHIDO, e o microfone embutido que assumiu o lugar dela como EM USO. Uma
 * tela que desenhasse só o primeiro chamaria a escolha de realidade.
 *
 * É também o que substitui o `SALVAR` do comp: não há o que confirmar quando a
 * tela já diz o que está valendo.
 *
 * A marca é texto, e não só a barra laranja: `specs/05-cliente-tui.md` proíbe
 * informação transmitida só por cor.
 */
function marcarLinhas(snapshot) {
  const aberto = snapshot?.capture?.id ?? null;

  for (const botao of $("lista-microfones").querySelectorAll(".dogma-dispositivo")) {
    const id = botao.dataset.dispositivo === "" ? null : botao.dataset.dispositivo;
    const escolhido = microfoneEscolhido === id;
    botao.dataset.escolhido = escolhido ? "sim" : "nao";

    let marca = "";
    if (aberto !== null && aberto === id) marca = "EM USO";
    else if (escolhido) marca = "ESCOLHIDO";
    else if (botao.dataset.padrao === "sim") marca = "PADRÃO";

    const alvo = botao.querySelector(".dogma-dispositivo-marca");
    if (alvo) alvo.textContent = marca;
  }
}

/**
 * O medidor de entrada, do mesmo `input_level` que a telemetria da sessão lê.
 *
 * Sem sessão não há nível. Um medidor parado em zero diria "seu microfone não
 * capta nada", que é uma frase diferente e falsa — daí o travessão.
 */
function desenharNivel(snapshot) {
  const medidor = $("dogma-nivel");
  if (!snapshot || !snapshot.audio_available) {
    medidor.dataset.vivo = "nao";
    medidor.textContent = "— SEM SESSÃO DE ÁUDIO";
    return;
  }

  const nivel = Math.max(0, Math.min(1, snapshot.telemetry.input_level));
  const cheios = Math.round(nivel * BLOCOS_DO_MEDIDOR);
  medidor.dataset.vivo = "sim";
  // O número acompanha os blocos pela mesma regra que a marca acompanha a cor:
  // a forma sozinha não é legível em monocromático nem por leitor de tela.
  medidor.textContent =
    "█".repeat(cheios) + "░".repeat(BLOCOS_DO_MEDIDOR - cheios) + ` ${Math.round(nivel * 100)}%`;
}

/**
 * Qual dos três modos do microfone está valendo.
 *
 * Três e não a chave de sim ou não do comp. `specs/03-audio.md` faz da tecla o
 * padrão porque ela nunca dispara sozinha, e uma chave de dois lados esconderia
 * o terceiro estado — o aberto, que é justamente o que ninguém quer ligar sem
 * saber. `aria-pressed` carrega a escolha para quem não vê o preenchimento.
 *
 * Sem sessão os três ficam apagados: `set_voice_mode` fala com um plug inserido,
 * e não há preferência em disco que os lembre. Apagado **e** dizendo por quê, ou
 * a lacuna se lê como defeito.
 */
function desenharModos(snapshot) {
  const semSessao = !snapshot;
  for (const botao of document.querySelectorAll(".dogma-modo")) {
    botao.disabled = semSessao;
    botao.setAttribute("aria-pressed", !semSessao && snapshot.voice_mode === botao.dataset.modo);
    if (semSessao) {
      botao.title = "Como o microfone abre é da sessão: sem plug inserido não há microfone para abrir.";
    } else {
      botao.removeAttribute("title");
    }
  }
}

/**
 * O apelido em uso.
 *
 * O único dado de identidade que esta janela alcança sem um comando novo. A
 * chave em si está em disco (ADR 0017) e nada a lê para cá — daí não haver tipo,
 * impressão digital nem data nesta seção, e nem molduras vazias no lugar delas.
 *
 * Sem sessão não há apelido reivindicado, e o travessão é a resposta certa: o
 * nome que se digita na entrada só vira o seu depois que o CASPER o vincula.
 */
function desenharIdentidade(snapshot) {
  const alvo = $("dogma-apelido");
  const apelido = snapshot?.nickname ?? "";
  alvo.textContent = apelido === "" ? "——" : apelido;
  alvo.classList.toggle("ausente", apelido === "");
}

// --------------------------------------------------------------------- ações

/** Escolhe um microfone, ou volta para o padrão da máquina com `null`. */
async function escolher(id) {
  const erro = $("dogma-erro");
  erro.hidden = true;
  try {
    await invoke("escolher_microfone", { dispositivo: id });
  } catch (falha) {
    // Revelar antes de escrever: `role="alert"` não anuncia o que já estava na
    // página enquanto ela estava escondida.
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
  }
  await desenharMicrofones();
  await atualizarDogma();
}

/** Troca o modo do microfone. O nome do modo volta como chegou. */
async function escolherModo(modo) {
  try {
    await invoke("set_voice_mode", { mode: modo });
  } catch (falha) {
    console.warn("set_voice_mode:", falha);
  }
  await atualizarDogma();
}

/** Abre a configuração, lembrando de onde. */
async function abrirDogma(origem) {
  telaDeOrigem = origem;
  $(origem).hidden = true;
  $("tela-dogma").hidden = false;
  $("dogma-erro").hidden = true;
  $("dogma-voltar-texto").textContent = VOLTA[origem] ?? "VOLTAR";
  await desenharMicrofones();
  await atualizarDogma();
}

/** Fecha e devolve para a tela que a abriu. */
function fecharDogma() {
  $("tela-dogma").hidden = true;
  const volta = telaDeOrigem ?? "tela-boot";
  $(volta).hidden = false;
  telaDeOrigem = null;
  // Só quando se volta para a sessão: `desenharMensagens` sai cedo enquanto a
  // lista está sem layout, e redesenhar aqui é o outro lado desse acordo. Vindo
  // da tela de entrada não há sessão nenhuma a redesenhar.
  if (volta === "tela-sessao") {
    atualizar().catch((falha) => console.warn("voltar do Terminal Dogma:", falha));
  }
}

/**
 * Some sem devolver ninguém.
 *
 * Para quem já está trocando de tela por cima: a sessão pode acabar enquanto
 * esta tela está aberta — um operador derruba, o enlace descarrega —, e
 * `mostrarFim` escolhe a tela seguinte por conta própria. Fechar de volta para a
 * origem ali reabriria a sessão que acabou de terminar; não fechar deixaria duas
 * telas empilhadas, porque toda `.tela` tem a altura da janela.
 */
function abandonarDogma() {
  $("tela-dogma").hidden = true;
  telaDeOrigem = null;
}

/**
 * Puxa o snapshot para o medidor, para quem está capturando, para os modos do
 * microfone e para o apelido.
 *
 * Sem sessão isto falha, e falhar é o estado normal desta tela quando aberta da
 * entrada — não é aviso de nada.
 */
async function atualizarDogma() {
  let snapshot = null;
  try {
    snapshot = await invoke("snapshot");
  } catch (falha) {
    if (falha !== "NotConnected") console.warn("snapshot:", falha);
  }
  desenharNivel(snapshot);
  marcarLinhas(snapshot);
  desenharModos(snapshot);
  desenharIdentidade(snapshot);
}

// ------------------------------------------------------------------- ligação

$("botao-dogma").addEventListener("click", () => abrirDogma("tela-boot"));
$("botao-dogma-sessao").addEventListener("click", () => abrirDogma("tela-sessao"));
$("dogma-fechar").addEventListener("click", fecharDogma);

for (const botao of document.querySelectorAll(".dogma-secao")) {
  botao.addEventListener("click", () => abrirSecao(botao.id));
}

for (const botao of document.querySelectorAll(".dogma-modo")) {
  botao.addEventListener("click", () => escolherModo(botao.dataset.modo));
}

// As legendas simples moram em `base.js`, e é de lá que vêm o `localStorage` e a
// classe no `body`. Este botão é a única coisa em todo o app que as liga e
// desliga; ele lê o estado de lá, e nunca guarda uma cópia.
$("dogma-legendas").addEventListener("click", () => {
  const ligado = !legendasSimples();
  aplicarLegendas(ligado);
  $("dogma-legendas").setAttribute("aria-checked", ligado);
});

// Escape fecha, que é o que uma tela sobreposta faz. Só com ela na frente, ou
// engoliria a tecla de quem está fechando uma busca na sessão.
window.addEventListener("keydown", (evento) => {
  if (evento.key === "Escape" && !$("tela-dogma").hidden) {
    evento.preventDefault();
    fecharDogma();
  }
});

// O estado inicial da tela, escrito uma vez com ela ainda escondida: a seção de
// áudio aberta e o interruptor de legendas no que `base.js` já aplicou ao
// `body`. Nenhum quadro chega a mostrar o cabeçalho vazio.
abrirSecao("secao-audio");
$("dogma-legendas").setAttribute("aria-checked", legendasSimples());

// O nível de entrada muda sozinho, e é a única coisa viva nesta tela. Mesmo
// meio segundo da telemetria da sessão, e só com a tela na frente: um `invoke`
// duas vezes por segundo para uma tela escondida é uma volta de IPC por nada.
setInterval(() => {
  if (!$("tela-dogma").hidden) atualizarDogma();
}, 500);
