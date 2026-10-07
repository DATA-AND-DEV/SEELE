// O retrato da pessoa aparece na grade da chamada, e não só nas mensagens.
await entrarNaSessao();
await abrirChamada();
await espera(800);
const cartoes = [...document.querySelectorAll(".chamada-cartao")];
relatar("cartões: " + cartoes.length);
for (const cartao of cartoes) {
  const avatar = cartao.querySelector(".chamada-avatar");
  const com = avatar.dataset.comRetrato === "sim";
  relatar(`${cartao.dataset.pessoa}: ${com ? "com retrato" : "iniciais " + avatar.textContent}`);
}
