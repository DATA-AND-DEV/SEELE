// A seção ATUALIZAÇÃO, aberta e com o resultado na tela.
document.getElementById("botao-server").click();
await espera(200);
document.getElementById("secao-atualizacao").click();
await espera(200);
document.getElementById("atualizacao-procurar").click();
await espera(400);
relatar("painel-atualizacao: " + visivel("painel-atualizacao"));
const estado = document.getElementById("atualizacao-estado");
relatar("estado: " + (estado.textContent || "vazio"));
