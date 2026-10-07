// Alinhamento e tamanho dos controles que a vista compara.
const cx = (id) => {
  const el = document.getElementById(id);
  if (!el) return `${id}=AUSENTE`;
  const r = el.getBoundingClientRect();
  return `${id}=${Math.round(r.width)}x${Math.round(r.height)}@y${Math.round(r.top)}`;
};
relatar("entrada: " + ["boot-perfil", "botao-server"].map(cx).join(" "));
await entrarNaSessao();
relatar("cabecalho: " + ["nova-voice_room", "ajuda-abrir"].map(cx).join(" "));
// O alternador `CONVERSA`/`CHAMADA` saiu do rodapé; a navegação é clicar numa
// sala ou num canal. Ver `rodape-do-operador.js`, que mede o rodapé novo.
const naSala = document.querySelector("#lista-voice_rooms button[data-dentro='nao']");
if (naSala) naSala.click();
await espera(600);
relatar("depois de entrar na sala: vista-chamada " + visivel("vista-chamada"));
