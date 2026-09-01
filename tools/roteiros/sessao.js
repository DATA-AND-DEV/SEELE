// A sessão pintada com a mesma massa que a comp desenha, para a foto poder ser
// posta ao lado dela. O diálogo da porta fecha porque ele cobre a tela — ele
// tem roteiro próprio se for o assunto.
document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) {
  document.getElementById("porta-entendi").click();
  await espera(200);
}
relatar(telas("sessao"));
relatar("porta: " + visivel("porta"));
