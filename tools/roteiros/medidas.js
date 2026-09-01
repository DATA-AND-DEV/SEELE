// Alinhamento e tamanho dos controles que a vista compara.
const cx = (id) => {
  const el = document.getElementById(id);
  if (!el) return `${id}=AUSENTE`;
  const r = el.getBoundingClientRect();
  return `${id}=${Math.round(r.width)}x${Math.round(r.height)}@y${Math.round(r.top)}`;
};
relatar("entrada: " + ["boot-perfil", "botao-server"].map(cx).join(" "));
document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) { document.getElementById("porta-entendi").click(); await espera(200); }
relatar("cabecalho: " + ["nova-voice_room", "ajuda-abrir"].map(cx).join(" "));
relatar("alternador: " + document.getElementById("operador-vista").textContent);
document.getElementById("operador-vista").click();
await espera(300);
relatar("depois do clique: " + document.getElementById("operador-vista").textContent
  + " (vista-chamada " + visivel("vista-chamada") + ")");
