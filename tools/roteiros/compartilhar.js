// A caixa de compartilhar pergunta qual monitor, e mais nada.
document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) { document.getElementById("porta-entendi").click(); await espera(200); }
await abrirChamada();
await espera(300);
document.getElementById("chamada-compartilhar").click();
await espera(500);
relatar("caixa: " + visivel("compartilhar"));
for (const id of ["compartilhar-prioridade", "compartilhar-altura", "compartilhar-quadros", "compartilhar-banda"]) {
  relatar(id + ": " + (document.getElementById(id) ? "AINDA EXISTE" : "fora"));
}
