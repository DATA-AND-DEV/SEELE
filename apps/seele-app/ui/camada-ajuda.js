/* SEELE — a ajuda, na tecla `?`.
 *
 * # Por que a camada continua, com metade do que tinha
 *
 * Ela nasceu para traduzir um vocabulário inteiro — as palavras do tema, que
 * quem chega do Discord não tinha por onde adivinhar. A renomeação levou esse
 * trabalho embora: servidor, sala de voz, canal, apelido e mudo se explicam no
 * próprio rótulo, e um verbete que repete o rótulo é ruído.
 *
 * O que sobra é o que **nenhum** rótulo cabe: conexão segura, que é uma
 * promessa sobre impressões digitais e não uma cor, e o sinal, cuja escala vale
 * dizer. Esses dois não têm nome autoexplicativo em português nenhum.
 *
 * E a seção das teclas, que é a outra metade e não encolheu: o critério que a
 * TUI adota para o `connection` é «dá para conectar sabendo só a tecla `?`», e esta
 * camada é o equivalente do cliente gráfico. O preço dela na tela permanente
 * continua sendo zero — ela não existe até ser chamada.
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
fecharAoClicarFora("ajuda", fecharAjuda);
$("ajuda-abrir").addEventListener("click", abrirAjuda);

/*
 * A tecla.
 *
 * `?` e não `F1`: é a tecla que o `connection` usa, e a paridade entre as duas cascas
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
