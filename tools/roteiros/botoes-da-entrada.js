// Os três botões da entrada abrem o que prometem.
//
// Um deles — `CONECTAR` — ficou sem responder por três versões: uma chamada
// morta no topo do `tela-boot.js` estourava antes de o `click` ser registrado.
for (const [botao, camada] of [
  ["botao-conectar", "servidores"],
  ["boot-perfil", "perfil"],
  ["botao-server", "tela-server"],
]) {
  document.getElementById(botao).click();
  await espera(150);
  relatar(`${botao} -> ${camada}: ${visivel(camada)}`);
  document.getElementById(camada).hidden = true;
}
