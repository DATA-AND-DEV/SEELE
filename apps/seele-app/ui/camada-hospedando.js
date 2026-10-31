// SEELE — hospedando (`#hospedando`, 0.19): o que a janela mostra do clique em
// HOSPEDAR até a sessão aparecer.
//
// **O que acontecia antes.** `hospedar()` (`tela-boot.js`) apagava o HOSPEDAR
// AQUI e esperava o `hospedar` do Rust — a lista de revogação, a cópia de
// segurança, as escolhas de MOD, o servidor e a escada da porta (UPnP, PCP e o
// ponto de encontro, com prazos de 1 a 3 s cada) —, e a entrada ficava parada,
// sem uma linha dizendo nada, durante esses segundos. A linha de etapa da
// entrada só acordava depois, no `connect`. «Tela de loading quando clicar em
// hospedar um server», pediu o dono.
//
// **Dois passos, e só os que esta janela vê fechar.** O `hospedar` do Rust é
// uma chamada só, sem andamento por dentro: inventar «cópia feita», «porta
// aberta» seria desenhar um progresso que ninguém mediu. O primeiro passo anda
// enquanto ela não volta, e fecha com o alcance que ela devolveu; o segundo é o
// `connect`, com as etapas de verdade (`ConnectStageChanged`, as mesmas da
// linha da entrada).
//
// **E a falha é dita aqui**, com a frase dela e o que ficou: se o servidor
// ficou no ar ou não é perguntado ao Rust (`meu_servidor`) depois da falha, e
// não suposto. «O produto sabe e não conta» era o defeito; a caixa conta.

"use strict";

/** Quem tinha o teclado antes da caixa abrir, para a falha o devolver. */
let focoAntesDeHospedando = null;

/** Abre a caixa no primeiro passo. Chamada por `hospedar()` antes do `invoke`. */
function abrirHospedando() {
  const f = FRASES_DA_HOSPEDAGEM.carregando;
  const camada = $("hospedando");
  focoAntesDeHospedando = document.activeElement;
  camada.dataset.fase = "subindo";
  $("hospedando-caixa").setAttribute("aria-busy", "true");
  $("hospedando-rotulo").textContent = f.rotulo;
  $("hospedando-titulo").textContent = f.titulo.subindo;
  marcarPassoDeHospedar("subir", "andando", f.subir, f.subirAndando);
  marcarPassoDeHospedar("entrar", "antes", f.entrar, f.entrarAntes);
  $("hospedando-erro").hidden = true;
  $("hospedando-erro").textContent = "";
  $("hospedando-nota").textContent = f.nota;
  $("hospedando-fechar").hidden = true;
  camada.hidden = false;
  // O que está atrás não recebe clique nem teclado: um CONECTAR ou um segundo
  // HOSPEDAR por baixo desta caixa abriria outra entrada no meio desta.
  $("tela-boot").inert = true;
  $("hospedando-caixa").focus();
  anunciar(f.titulo.subindo);
}

function marcarPassoDeHospedar(passo, estado, nome, detalhe) {
  const item = $(`hospedando-${passo}`);
  item.dataset.estado = estado;
  if (nome !== undefined) $(`hospedando-${passo}-nome`).textContent = nome;
  $(`hospedando-${passo}-detalhe`).textContent = detalhe ?? "";
}

/** O `hospedar` do Rust voltou: o servidor está no ar, e a entrada começa. */
function servidorDeHospedarSubiu(alcance) {
  const f = FRASES_DA_HOSPEDAGEM.carregando;
  if ($("hospedando").hidden) return;
  $("hospedando").dataset.fase = "entrando";
  $("hospedando-titulo").textContent = f.titulo.entrando;
  // O alcance é o que o `hospedar` devolveu: a frase do degrau em que a escada
  // parou, a mesma que a porta mostra.
  marcarPassoDeHospedar("subir", "feito", undefined, alcance ? `${f.subirFeito} ${alcance}` : f.subirFeito);
  marcarPassoDeHospedar("entrar", "andando", undefined, f.entrarAndando);
}

/**
 * Uma etapa da chegada (`mostrarEtapa`, em `tela-boot.js`), escrita também no
 * segundo passo enquanto a caixa está na frente e entrando.
 */
function etapaDeHospedar(frase) {
  if (frase === null || $("hospedando").hidden || $("hospedando").dataset.fase !== "entrando") return;
  $("hospedando-entrar-detalhe").textContent = frase;
}

/** Fecha a caixa e devolve a entrada ao clique e ao teclado. */
function fecharHospedando() {
  const camada = $("hospedando");
  const tinhaOFoco = camada.contains(document.activeElement);
  camada.hidden = true;
  delete camada.dataset.fase;
  $("tela-boot").inert = false;
  // Só devolve o teclado se ele estava aqui: com a sessão aberta, ou com a
  // porta ou o aceite dos MODs na frente, o foco já é de quem abriu depois.
  if (tinhaOFoco && !$("tela-boot").hidden) {
    if (focavel(focoAntesDeHospedando)) focoAntesDeHospedando.focus();
    else abrirTela("tela-boot");
  }
  focoAntesDeHospedando = null;
}

/**
 * Um passo falhou: o título diz qual, a frase diz o quê, e a nota diz o que
 * ficou. O FECHAR é a saída, e a entrada continua com a linha de erro dela.
 */
function hospedagemFalhou(passo, frase, nota) {
  const f = FRASES_DA_HOSPEDAGEM.carregando;
  const camada = $("hospedando");
  if (camada.hidden) return;
  camada.dataset.fase = "falhou";
  $("hospedando-caixa").setAttribute("aria-busy", "false");
  $("hospedando-titulo").textContent = passo === "subir" ? f.titulo.naoSubiu : f.titulo.naoEntrou;
  marcarPassoDeHospedar(passo, "falhou", undefined, "");
  $("hospedando-erro").textContent = frase;
  $("hospedando-erro").hidden = false;
  $("hospedando-nota").textContent = nota;
  const fechar = $("hospedando-fechar");
  fechar.textContent = f.fechar;
  fechar.hidden = false;
  // O teclado vem para a saída se estava com a caixa (ou com ninguém); uma
  // pergunta aberta por cima (a porta, o nome) continua com ele.
  const ativo = document.activeElement;
  if (!ativo || ativo === document.body || camada.contains(ativo)) fechar.focus();
}

$("hospedando-fechar").addEventListener("click", fecharHospedando);

// Escape fecha só a falha: enquanto anda, não há o que fechar — o servidor
// estaria subindo por baixo de uma caixa que sumiu.
window.addEventListener("keydown", (evento) => {
  if (evento.key !== "Escape" || $("hospedando").hidden || $("hospedando").dataset.fase !== "falhou") return;
  if (!$("porta").hidden) return;
  evento.preventDefault();
  fecharHospedando();
});
