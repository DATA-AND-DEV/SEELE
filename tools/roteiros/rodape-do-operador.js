// O rodapé do operador, que muda de ofício com a sala.
//
// O que os guardas de texto não alcançam: se o botão de compartilhar realmente
// some fora de sala, se o de sair troca de rótulo, e se clicar num canal leva
// mesmo para a conversa. Os três são estado de DOM depois de um quadro, e não
// uma string dentro de um arquivo.
//
// Substitui `alternador.js`, que media o rótulo `CONVERSA`/`CHAMADA` de um botão
// que não existe mais: a navegação passou para as listas da esquerda.

function rodape() {
  const sair = document.getElementById("operador-sair");
  const outro = document.getElementById("operador-vista");
  return (
    sair.textContent.trim() +
    " | " +
    (outro.hidden ? "(escondido)" : outro.textContent.trim())
  );
}

document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) { document.getElementById("porta-entendi").click(); await espera(200); }

await espera(700);
relatar("recém-chegado: " + rodape());

// Entrar na sala é apertar a fileira dela, que é a navegação nova.
const entrar = document.querySelector("#lista-voice_rooms button[data-dentro='nao']");
relatar("fileira fora da sala: " + (entrar ? entrar.textContent.trim() : "NÃO ACHEI"));
if (entrar) entrar.click();
await espera(900);
relatar("dentro da sala: " + rodape() + " (vista-chamada " + visivel("vista-chamada") + ")");

const dentro = document.querySelector("#lista-voice_rooms button[data-dentro='sim']");
relatar("fileira dentro da sala: " + (dentro ? dentro.textContent.trim() : "NÃO ACHEI"));

// E clicar num canal devolve para a conversa, sem sair da sala.
const canal = document.querySelector("#lista-linhas button[data-linha]");
if (canal) canal.click();
await espera(700);
relatar("depois de escolher um canal: vista-chamada " + visivel("vista-chamada")
  + " · rodapé " + rodape());

// De volta para a grade pela própria fileira da sala.
const voltar = document.querySelector("#lista-voice_rooms button[data-dentro='sim']");
if (voltar) voltar.click();
await espera(700);
relatar("depois de apertar a sala: vista-chamada " + visivel("vista-chamada"));

// O lápis do canal, ao lado do ×, empilhado.
const controles = document.querySelector("#lista-linhas .linha-controles");
relatar("controles do canal: " + (controles
  ? [...controles.children].map((b) => b.className).join(" + ")
  : "NENHUM (sem permissão, ou some)"));
