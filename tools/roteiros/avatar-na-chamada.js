// O retrato da pessoa aparece na grade da chamada, e não só nas mensagens.
document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) { document.getElementById("porta-entendi").click(); await espera(200); }
await abrirChamada();
await espera(800);
const cartoes = [...document.querySelectorAll(".chamada-cartao")];
relatar("cartões: " + cartoes.length);
for (const cartao of cartoes) {
  const avatar = cartao.querySelector(".chamada-avatar");
  const com = avatar.dataset.comRetrato === "sim";
  relatar(`${cartao.dataset.pessoa}: ${com ? "com retrato" : "iniciais " + avatar.textContent}`);
}
