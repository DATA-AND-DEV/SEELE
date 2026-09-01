// «Quando clico no meio dos botões de microfone e de fone não funciona.
//  Precisa clicar no cantinho do botão.»
//
// A pergunta é literal: o que o navegador encontra no centro do botão?
document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) { document.getElementById("porta-entendi").click(); await espera(200); }
await abrirChamada();
await espera(600);

for (const id of ["botao-mudo", "botao-surdo", "botao-server-sessao"]) {
  const alvo = document.getElementById(id);
  if (!alvo) { relatar(id + ": não existe"); continue; }
  const r = alvo.getBoundingClientRect();
  if (r.width === 0) { relatar(id + ": invisível"); continue; }
  const nomeDe = (el) => el ? (el.id ? "#" + el.id : el.tagName.toLowerCase() + "." + (el.className.baseVal ?? el.className ?? "")) : "nada";
  const centro = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
  const canto = document.elementFromPoint(r.left + 2, r.top + 2);
  relatar(
    id + " " + Math.round(r.width) + "x" + Math.round(r.height) +
    " | centro: " + nomeDe(centro) +
    " | canto: " + nomeDe(canto) +
    " | centro está dentro do botão? " + (alvo.contains(centro) ? "sim" : "NÃO")
  );
}
