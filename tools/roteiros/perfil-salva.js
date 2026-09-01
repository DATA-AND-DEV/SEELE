// O nome digitado no perfil da tela inicial tem de ser gravado.
//
// Ele não era: a decisão de «estou numa sessão?» vinha da visibilidade do bloco
// da imagem, que passou a ser sempre visível quando os dois diálogos viraram um.
// A entrada chamava o comando de sessão, que não tem sessão, e o nome sumia.
document.getElementById("boot-perfil").click();
await espera(400);
const campo = document.getElementById("perfil-apelido");
campo.value = "aleta";
const antes = window.__SEELE_CHAMADAS.length;
document.getElementById("perfil-salvar").click();
await espera(400);
relatar("comandos: " + window.__SEELE_CHAMADAS.slice(antes).join(",") );
relatar("perfil depois de salvar: " + visivel("perfil"));
