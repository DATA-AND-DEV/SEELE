// Quem cola um código estando numa sessão lê «ligar sai de» antes de sair dela
// (tarefa 30 da F7; spec de 23/09, «Do lado de quem liga», 2). A metade da
// entrada é `ligar.js`.
//
// 1. **De dentro da sessão, um código abre a caixa LIGAR**, com «ligar para X
//    sai de» e o nome deste servidor, e nada sai antes do sim. A sessão é a do
//    próprio servidor (HOSPEDAR AQUI), e a caixa diz também que ele continua.
// 2. **No sim, liga para o alvo `@CÓDIGO`.**

const CODIGO = "CSM7-NBFR-CAYQ-EV4Y";
const ALVO = "@" + CODIGO;

const chamou = (cmd) => window.__SEELE_CHAMADAS.includes(cmd);
const colar = async (texto, pronto) => {
  await abrirServidores();
  document.getElementById("servidores-endereco").value = texto;
  window.__SEELE_CHAMADAS.length = 0;
  document.getElementById("servidores-forma").dispatchEvent(
    new Event("submit", { cancelable: true }),
  );
  await ate(pronto, 800);
};

await entrarNaSessao();

// 1. De dentro da sessão, um código: a caixa LIGAR.
SEELE_RESPOSTAS.ler_entrada = { Codigo: { codigo: CODIGO, alvo: ALVO } };
await colar(CODIGO, () => visivel("moderar") === "VISIVEL");
const titulo = document.getElementById("moderar-titulo").textContent;
const frase = document.getElementById("moderar-consequencia").textContent;
const botao = document.getElementById("moderar-confirmar").textContent;
relatar(`a caixa: ${titulo} · «${frase.replace(/\s+/g, " ")}» · ${botao}`);
exigir(titulo === "LIGAR", `a caixa de ligar de dentro de uma sessão é «${titulo}»`);
exigir(frase.includes(`Ligar para ${CODIGO} sai de`) && frase.includes("TÓQUIO-3"),
  `a caixa não diz que ligar sai deste servidor: «${frase}»`);
exigir(botao === `LIGAR PARA ${CODIGO}`, `o botão da caixa é «${botao}»`);
exigir(frase.includes("continua no ar"),
  `ligar de dentro do próprio servidor não diz que ele continua no ar: «${frase}»`);
exigir(!chamou("connect") && !chamou("disconnect"), "ligar de dentro da sessão não esperou o sim");

// 2. No sim, liga.
delete window.__SEELE_ARGS.connect;
document.getElementById("moderar-confirmar").click();
await ate(() => window.__SEELE_ARGS.connect?.server === ALVO, 1500);
relatar(`no sim, ligou para: ${window.__SEELE_ARGS.connect?.server}`);
exigir(window.__SEELE_ARGS.connect?.server === ALVO,
  `o sim da caixa não ligou para o alvo: ${window.__SEELE_ARGS.connect?.server}`);
