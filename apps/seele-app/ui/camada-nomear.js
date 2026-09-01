// Nomear: um diálogo para dar nome a qualquer coisa.
//
// # Por que um só, e não um por caso
//
// Três coisas passam por aqui — criar sala de voz, criar canal, renomear sala —
// e as três pedem exatamente o mesmo de quem usa: um nome curto, um Enter, e um
// motivo por escrito quando o servidor recusa. O que muda entre elas são
// palavras, e palavras não justificam três marcações, três armadilhas de foco e
// três lugares para o `maxlength` divergir.
//
// # O que ele substitui
//
// Os dois formulários `criar` que moravam **abertos** na coluna da esquerda. A
// comp da 0.9.0 os troca por um `+` no cabeçalho de cada lista, e o ganho não é
// de espaço: um formulário sempre visível numa coluna de navegação é um convite
// a preencher, e criar canal não é o que se faz ali toda hora.
//
// # O que este arquivo não decide
//
// O que acontece com o nome. Quem abre passa um `aoConfirmar`, e é dele o
// comando, a recusa e o que fazer depois. Este arquivo sabe abrir, fechar,
// devolver o foco e escrever o motivo — e mais nada.

"use strict";

/** Para onde o teclado volta quando o diálogo fecha. */
let focoAntesDeNomear = null;

/** O que fazer com o nome, posto por quem abriu. */
let aoConfirmarNome = null;

/**
 * Abre o diálogo.
 *
 * @param {object} pedido
 * @param {string} pedido.titulo      o cabeçalho, em caixa alta
 * @param {string} pedido.rotulo      o que se está nomeando
 * @param {string} pedido.exemplo     o `placeholder`
 * @param {string} pedido.acao        o verbo do botão que confirma
 * @param {string} [pedido.valor]     o nome de agora, ao renomear
 * @param {(nome: string) => Promise<void>} pedido.aoConfirmar
 */
function abrirNomear(pedido) {
  focoAntesDeNomear = document.activeElement;
  aoConfirmarNome = pedido.aoConfirmar;

  $("nomear-titulo").textContent = pedido.titulo;
  $("nomear-rotulo").textContent = pedido.rotulo;
  $("nomear-confirmar").textContent = pedido.acao;
  $("nomear-erro").hidden = true;
  $("nomear-erro").textContent = "";

  const campo = $("nomear-valor");
  campo.placeholder = pedido.exemplo ?? "";
  campo.value = pedido.valor ?? "";

  $("nomear").hidden = false;
  campo.focus();
  // Selecionado, e não com o cursor no fim: ao renomear, o gesto seguinte é
  // quase sempre trocar o nome inteiro, e quem só quiser emendar aperta uma
  // seta. O contrário obrigaria todo mundo a apagar antes de escrever.
  campo.select();
  anunciar(`${pedido.titulo}. Escape fecha.`);
}

/**
 * Fecha, devolvendo o teclado a quem abriu.
 *
 * `focavel` antes do `focus()` pela mesma razão da ajuda: a lista de baixo pode
 * ter sido redesenhada enquanto o diálogo estava aberto — e um `focus()` num nó
 * que saiu do documento não faz nada e não avisa.
 */
function fecharNomear() {
  $("nomear").hidden = true;
  aoConfirmarNome = null;
  if (focavel(focoAntesDeNomear)) {
    focoAntesDeNomear.focus();
  }
  focoAntesDeNomear = null;
}

/** Diz, onde se acabou de digitar, por que o nome não passou. */
function recusarNome(frase) {
  const onde = $("nomear-erro");
  // Revelar antes de escrever: `role="alert"` não anuncia o que já estava na
  // página enquanto ela estava escondida.
  onde.hidden = false;
  onde.textContent = frase;
  $("nomear-valor").focus();
}

$("nomear-forma").addEventListener("submit", async (evento) => {
  evento.preventDefault();
  const nome = $("nomear-valor").value.trim();
  // Vazio não é pedido: é o Enter de quem ainda não escreveu. O campo continua
  // aberto e nada é mandado.
  if (nome === "") return;
  if (!aoConfirmarNome) return;

  const confirmar = $("nomear-confirmar");
  confirmar.disabled = true;
  try {
    await aoConfirmarNome(nome);
    fecharNomear();
  } catch (falha) {
    console.warn("nomear:", falha);
    recusarNome(fraseDeErro(falha));
  } finally {
    confirmar.disabled = false;
  }
});

$("nomear-cancelar").addEventListener("click", fecharNomear);
fecharAoClicarFora("nomear", fecharNomear);

// `Escape` fecha, e em fase de captura: com o diálogo aberto ele é a coisa mais
// de cima da janela, e a tecla não pode fechar a busca ou a gaveta atrás dele.
window.addEventListener(
  "keydown",
  (evento) => {
    if (evento.key !== "Escape" || $("nomear").hidden) return;
    evento.preventDefault();
    evento.stopPropagation();
    fecharNomear();
  },
  true,
);
