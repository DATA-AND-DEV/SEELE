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
  // E a configuração, que abre por cima da sessão. Toda `.tela` tem a altura da
  // janela, então duas visíveis não se sobrepõem: empilham, e a segunda fica
  // abaixo da dobra onde ninguém a encontra.
  abandonarDogma();
  // A chamada pela mesma razão, e ela é a mais provável das duas: um operador
  // que derruba alguém derruba quem está num Cage, que é exatamente quem está
  // olhando esta tela.
  abandonarChamada();
  $("tela-fim").hidden = false;
  const frase = MOTIVOS[motivo] ?? null;
  $("fim-motivo").textContent = frase ?? "ENLACE ENCERRADO";
  // A única troca de tela que ninguém pediu, e a única cujo anúncio carrega o
  // motivo: quem não vê a tela precisa do porquê junto, ou fica com um botão
  // EJETAR focado e nenhuma explicação de por que ele apareceu. Sem motivo
  // nomeado, o `data-anuncio` da tela já diz o que há para dizer — repeti-lo
  // aqui seria anunciar «enlace encerrado» duas vezes na mesma frase.
  abrirTela("tela-fim", frase ? `Enlace encerrado. ${frase}` : undefined);
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
  // Depois do redesenho, porque a lista de visitados acabou de mudar de
  // tamanho e o campo de endereço é o alvo desta tela.
  abrirTela("tela-boot");
});
