// O mesmo, olhando o ladrilho da trilha — que é onde o servidor se mostra na
// tela o tempo todo, e não só quando a configuração está aberta.
window.__SEELE_ICONE_DO_SERVER = [1, 2, 3, 4];
document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) { document.getElementById("porta-entendi").click(); await espera(200); }
const olhar = () => {
  const ladrilho = document.getElementById("trilha-server");
  const img = ladrilho.querySelector("img");
  return img ? "imagem de " + (img.src || "").length + " chars" : "sigla " + ladrilho.textContent;
};
relatar("antes: " + olhar());
window.__SEELE_ICONE_DO_SERVER = [9, 8, 7, 6, 5, 4, 3, 2, 1];
SEELE_QUADRO.icon_revision = 1;
window.__SEELE_EMITIR("ServerChanged");
await espera(900);
relatar("depois: " + olhar());
