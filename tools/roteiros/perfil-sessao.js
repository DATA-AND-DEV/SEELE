// O mesmo modal, visto de dentro de um servidor.
document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) { document.getElementById("porta-entendi").click(); await espera(200); }
document.getElementById("operador-quem").click();
await espera(400);
relatar("perfil: " + visivel("perfil"));
