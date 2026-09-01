// O nome escrito no perfil da tela inicial tem de chegar ao servidor que se
// sobe logo em seguida.
//
// Era o caminho que não lia a preferência: `hospedar` chama `conectar()` sem
// argumento, e só o diálogo de conhecidos buscava o apelido desta máquina — a
// única porta que o fazia, e a que quem hospeda não atravessa.
document.getElementById("boot-perfil").click();
await espera(400);
document.getElementById("perfil-apelido").value = "aleta";
document.getElementById("perfil-salvar").click();
await espera(400);
relatar("gravado nesta maquina: [" + (window.__SEELE_APELIDO ?? "(nada)") + "]");

document.getElementById("botao-hospedar").click();
await espera(900);
const args = window.__SEELE_ARGS.connect;
relatar("connect levou nickname=[" + ((args && args.nickname) ?? "(nunca chamou)") + "]");
