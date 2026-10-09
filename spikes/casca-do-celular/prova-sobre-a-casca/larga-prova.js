// A casca larga (D-web-3), do lado do script, sobre a casca de verdade.
//
// Duas trocas, as que a foto do spec mostra, e nenhuma lógica nova:
// - o propósito da entrada perde a metade «ou ponha o seu no ar a partir desta
//   máquina», que no navegador seria mentira;
// - HOSPEDAR AQUI vira a frase do app do SEELE, com o link para baixá-lo.
// As frases vão para o `frases.js` na Etapa B, e o link é a constante
// `PAGINA_DO_APP`.

"use strict";

(() => {
  if (document.documentElement.dataset.casca !== "larga") return;

  const PAGINA_DO_APP = "https://seele.app.br/#baixar";

  document.querySelector("#tela-boot .boot-proposito").textContent =
    "Voz, vídeo e texto entre pessoas, sem servidor de ninguém no meio. Entre no servidor de alguém pelo link que a pessoa mandou.";

  const frase = document.createElement("p");
  frase.className = "nota larga-hospedar";
  frase.append("Hospedar um servidor é pelo app do SEELE. ");
  const link = document.createElement("a");
  link.href = PAGINA_DO_APP;
  link.target = "_blank";
  link.rel = "noopener";
  link.textContent = "BAIXAR O APP DO SEELE";
  frase.append(link);
  document.getElementById("botao-hospedar").after(frase);
})();
