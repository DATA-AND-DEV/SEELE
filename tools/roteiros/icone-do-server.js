// Mudar o retrato do servidor tem de aparecer sem recarregar nada.
//
// O caminho é: o servidor difunde `ServerIconChanged`, o cliente sobe
// `icon_revision`, avisa `ServerChanged`, e a casca rebusca os bytes.
window.__SEELE_ICONE_DO_SERVER = [1, 2, 3, 4];
document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) { document.getElementById("porta-entendi").click(); await espera(200); }
document.getElementById("botao-server-sessao").click();
await espera(300);
document.getElementById("secao-servidor").click();
await espera(400);
const previa = document.getElementById("server-icone-previa");
relatar("antes: " + (previa.hidden ? "escondida" : "com imagem de " + (previa.src || "").length + " chars"));

// O servidor difundiu: bytes novos e revisão nova.
window.__SEELE_ICONE_DO_SERVER = [9, 8, 7, 6, 5, 4, 3, 2, 1];
SEELE_QUADRO.icon_revision = 1;
window.__SEELE_EMITIR("ServerChanged");
await espera(600);
relatar("depois: " + (previa.hidden ? "escondida" : "com imagem de " + (previa.src || "").length + " chars"));
