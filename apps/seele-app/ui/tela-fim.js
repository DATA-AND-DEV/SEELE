// SEELE · Entry Plug — a tela de sessão encerrada (`#tela-fim`).
//
// Diz por quê; um app que fecha calado vira suporte. Chega aqui por duas
// portas: um `ended` dentro do snapshot que `tela-sessao.js` projeta, e o
// evento `Ended` que o ouvinte de lá recebe. As duas chamam `mostrarFim`.

"use strict";

function mostrarFim(motivo) {
  $("tela-sessao").hidden = true;
  $("tela-boot").hidden = true;
  // A autenticação também: uma sessão pode acabar com o plug ainda fora, e
  // quem ficasse nela veria o Dogma acabar por trás de um botão que promete
  // entrar nele.
  $("tela-auth").hidden = true;
  $("tela-fim").hidden = false;
  $("fim-motivo").textContent = MOTIVOS[motivo] ?? "ENLACE ENCERRADO";
}

// ------------------------------------------------------------------- ligação

$("botao-voltar").addEventListener("click", async () => {
  await invoke("disconnect");
  $("tela-fim").hidden = true;
  $("tela-boot").hidden = false;
  // O `disconnect` também derruba o Dogma hospedado. A caixa some junto, ou
  // ficaria oferecendo um link que não leva mais a lugar nenhum.
  $("convite").hidden = true;
  mostrarVeredito(null);
  desenhado = null;
  linhaAberta = null;
  await encerrarBusca();
  limparConvite();
  subsistemas("", "·");
  await desenharVisitados();
});
