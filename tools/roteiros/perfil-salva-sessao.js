// O mesmo botão, dentro de um servidor, tem de mandar o nome ao servidor.
document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) { document.getElementById("porta-entendi").click(); await espera(200); }
document.getElementById("operador-quem").click();
await espera(400);
document.getElementById("perfil-apelido").value = "aleta";
const antes = window.__SEELE_CHAMADAS.length;
document.getElementById("perfil-salvar").click();
await espera(400);
relatar("comandos: " + window.__SEELE_CHAMADAS.slice(antes).join(","));
