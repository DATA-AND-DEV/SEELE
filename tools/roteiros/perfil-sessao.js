// O mesmo modal, visto de dentro de um servidor.
await entrarNaSessao();
document.getElementById("operador-quem").click();
await espera(400);
relatar("perfil: " + visivel("perfil"));
