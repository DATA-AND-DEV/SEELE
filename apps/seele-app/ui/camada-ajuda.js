/* SEELE · Entry Plug — a ajuda, na tecla `?`.
 *
 * # Por que uma camada e não legendas
 *
 * O vocabulário deste produto não se explica sozinho: Dogma, Piloto, Cage,
 * Linha e A.T. Field são palavras que quem chega do Discord não tem por onde
 * adivinhar. A resposta anterior eram notas sob os rótulos, e elas saíram por
 * decisão de desenho — legenda permanente é texto que quem já sabe lê mil
 * vezes, e este produto está sendo enxugado de texto.
 *
 * O critério que a TUI adota para o `plug` é «dá para conectar sabendo só a
 * tecla `?`». Esta camada é o equivalente do cliente gráfico, e o preço dela na
 * tela permanente é zero: ela não existe até ser chamada.
 */

/** Onde o teclado estava quando a ajuda abriu. */
let focoAntesDaAjuda = null;

/**
 * Abre a ajuda.
 *
 * `focus()` na caixa e não no primeiro termo: quem abriu quer ler, e um leitor
 * de tela que começa no meio da lista não anuncia o título que diz o que é
 * isto. `anunciar` porque revelar um nó não é evento nenhum para quem não vê a
 * tela — o mesmo argumento da portaria.
 */
function abrirAjuda() {
  focoAntesDaAjuda = document.activeElement;
  $("ajuda").hidden = false;
  $("ajuda").focus();
  anunciar("Ajuda. O vocabulário do SEELE e as teclas. Escape fecha.");
}

/**
 * Fecha, devolvendo o teclado a quem abriu.
 *
 * `focavel` antes de `focus()` pelo mesmo motivo da portaria: a ajuda pode ser
 * aberta de qualquer tela, e a tela de baixo pode ter trocado enquanto ela
 * estava aberta — um `focus()` num nó que saiu do documento não faz nada e não
 * reporta nada.
 */
function fecharAjuda() {
  $("ajuda").hidden = true;
  if (focavel(focoAntesDaAjuda)) {
    focoAntesDaAjuda.focus();
  }
  focoAntesDaAjuda = null;
}

$("ajuda-fechar").addEventListener("click", fecharAjuda);
$("ajuda-abrir").addEventListener("click", abrirAjuda);

/*
 * A tecla.
 *
 * `?` e não `F1`: é a tecla que o `plug` usa, e a paridade entre as duas cascas
 * é o ponto. Em teclado ABNT2 e US o `?` exige Shift, e é por isso que a
 * comparação é por `evento.key` — que já traz o caractere resolvido — e não por
 * `code`, que diria a tecla física e erraria em metade dos layouts.
 *
 * `digitando()` guarda a mesma coisa que guarda a `/` da busca e o espaço da
 * fala: uma interrogação escrita numa mensagem é uma interrogação.
 */
window.addEventListener("keydown", (evento) => {
  if (evento.key === "?" && !digitando() && $("ajuda").hidden) {
    evento.preventDefault();
    abrirAjuda();
    return;
  }
  // Escape fecha, como em toda coisa que se põe por cima nesta janela. Só com a
  // caixa na frente, ou engoliria a tecla de quem está fechando outra coisa.
  if (evento.key === "Escape" && !$("ajuda").hidden) {
    evento.preventDefault();
    fecharAjuda();
  }
});
