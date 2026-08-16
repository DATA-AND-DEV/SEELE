// SEELE · Entry Plug — o Terminal Dogma (`#tela-dogma`).
//
// A configuração local: o que é desta máquina e não deste Dogma. Hoje, qual
// microfone abrir — que era a lacuna registrada no inventário do comp, §15:
// "não há tela de configuração nenhuma".
//
// Alcançável das duas telas vivas, e volta para a que a abriu. Escolher
// microfone antes de conectar é tão comum quanto durante, e uma configuração
// atrás da sessão poria o controle atrás da porta que ele serve para abrir.
//
// ---- o que este arquivo não decide ----
//
// Nada. A lista de dispositivos vem do Rust, a escolha vai para o Rust, e é lá
// que ela é gravada e aplicada. Este arquivo não sabe o que é um id de
// dispositivo: ele o desenha nunca e o devolve inteiro.

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
    const vazio = elemento("li", "microfones-vazio", "ESTA MÁQUINA NÃO LISTOU DISPOSITIVO NENHUM");
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
 */
function linhaDeMicrofone(id, nome, ehPadrao) {
  const linha = elemento("li");
  const botao = elemento("button", "microfone");
  botao.type = "button";
  botao.dataset.dispositivo = id;
  botao.dataset.padrao = ehPadrao ? "sim" : "nao";
  botao.append(elemento("span", "microfone-nome", nome), elemento("span", "microfone-marca"));
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
 * A marca é texto, e não só a barra laranja: `specs/05-cliente-tui.md` proíbe
 * informação transmitida só por cor.
 */
function marcarLinhas(snapshot) {
  const aberto = snapshot?.capture?.id ?? null;

  for (const botao of $("lista-microfones").querySelectorAll(".microfone")) {
    const id = botao.dataset.dispositivo === "" ? null : botao.dataset.dispositivo;
    const escolhido = microfoneEscolhido === id;
    botao.dataset.escolhido = escolhido ? "sim" : "nao";

    let marca = "";
    if (aberto !== null && aberto === id) marca = "EM USO";
    else if (escolhido) marca = "ESCOLHIDO";
    else if (botao.dataset.padrao === "sim") marca = "PADRÃO";

    const alvo = botao.querySelector(".microfone-marca");
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

/** Abre a configuração, lembrando de onde. */
async function abrirDogma(origem) {
  telaDeOrigem = origem;
  $(origem).hidden = true;
  $("tela-dogma").hidden = false;
  $("dogma-erro").hidden = true;
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
 * Puxa o snapshot para o medidor e para quem está capturando.
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
}

// ------------------------------------------------------------------- ligação

$("botao-dogma").addEventListener("click", () => abrirDogma("tela-boot"));
$("botao-dogma-sessao").addEventListener("click", () => abrirDogma("tela-sessao"));
$("dogma-fechar").addEventListener("click", fecharDogma);

// Escape fecha, que é o que uma tela sobreposta faz. Só com ela na frente, ou
// engoliria a tecla de quem está fechando uma busca na sessão.
window.addEventListener("keydown", (evento) => {
  if (evento.key === "Escape" && !$("tela-dogma").hidden) {
    evento.preventDefault();
    fecharDogma();
  }
});

// O nível de entrada muda sozinho, e é a única coisa viva nesta tela. Mesmo
// meio segundo da telemetria da sessão, e só com a tela na frente: um `invoke`
// duas vezes por segundo para uma tela escondida é uma volta de IPC por nada.
setInterval(() => {
  if (!$("tela-dogma").hidden) atualizarDogma();
}, 500);
