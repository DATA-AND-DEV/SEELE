// A barra de janela como o macOS a desenha: os semáforos do sistema à esquerda,
// e a marca SEELE à direita.
document.getElementById("barra-janela").dataset.plataforma = "macos";
document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) { document.getElementById("porta-entendi").click(); await espera(200); }
const barra = document.getElementById("barra-janela");
const caixas = [...barra.children].map((el) => {
  const r = el.getBoundingClientRect();
  return `${el.className || el.tagName}[${Math.round(r.left)}..${Math.round(r.right)}]`;
});
relatar("barra: " + caixas.join(" "));
