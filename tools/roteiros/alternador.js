// O rótulo tem de seguir a vista mesmo quando quem a abre não é o botão.
// Compartilhar a tela chama `abrirChamada` por conta própria, e era esse o
// caminho em que a palavra ficava velha.
document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) { document.getElementById("porta-entendi").click(); await espera(200); }
relatar("na conversa: " + document.getElementById("operador-vista").textContent);
await abrirChamada();
relatar("logo depois de abrirChamada(): " + document.getElementById("operador-vista").textContent);
await espera(700);
relatar("depois de um quadro: " + document.getElementById("operador-vista").textContent
  + " (vista-chamada " + visivel("vista-chamada") + ")");
