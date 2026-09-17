// Entrar num servidor que roda outra versão abre **aquela** versão — ADR 0046.
//
// É a frase inteira da decisão, e o que este roteiro prova é que ela acontece
// **antes de conectar**: a versão vem no `seele://`, e não no fio, porque um
// servidor de uma versão anterior pode falar um protocolo que este cliente já
// não alcança — ele seria recusado com «versão incompatível» sem chegar a dizer
// uma palavra sobre si.

const LINK =
  "seele://casa.exemplo:8383/?fp=" + "a".repeat(64) + "&v=0.10.5";

// O que o Rust responde ao ler aquele link: o servidor roda a 0.10.5, e ela
// está instalada aqui.
SEELE_RESPOSTAS.analisar_convite = {
  alvo: "casa.exemplo:8383",
  token: null,
  versao: "0.10.5",
  pode_abrir_naquela_versao: true,
};

document.getElementById("botao-conectar").click();
await espera(150);
document.getElementById("servidores-endereco").value = LINK;
window.__SEELE_CHAMADAS.length = 0;
document.getElementById("servidores-forma").dispatchEvent(
  new Event("submit", { cancelable: true }),
);
await espera(300);

const pedido = window.__SEELE_ARGS.abrir_versao ?? {};
relatar(`abriu outra versão: ${window.__SEELE_CHAMADAS.includes("abrir_versao")}`);
relatar(`versão pedida: ${pedido.versao}`);
relatar(`levou o link inteiro: ${pedido.link === LINK}`);
relatar(`pediu para hospedar: ${pedido.hospedar}`);
// A metade que importa tanto quanto: **não** conectou nesta versão.
relatar(`conectou aqui mesmo: ${window.__SEELE_CHAMADAS.includes("connect")}`);
relatar(`diálogo de servidores: ${visivel("servidores")}`);

// E o outro lado: um servidor da mesma versão não desvia nada.
SEELE_RESPOSTAS.analisar_convite = {
  alvo: "casa.exemplo:8383",
  token: null,
  versao: "0.11.0",
  pode_abrir_naquela_versao: false,
};
document.getElementById("botao-conectar").click();
await espera(150);
document.getElementById("servidores-endereco").value = LINK;
window.__SEELE_CHAMADAS.length = 0;
document.getElementById("servidores-forma").dispatchEvent(
  new Event("submit", { cancelable: true }),
);
await espera(300);
relatar(`mesma versão, abriu outra: ${window.__SEELE_CHAMADAS.includes("abrir_versao")}`);
relatar(`mesma versão, conectou: ${window.__SEELE_CHAMADAS.includes("connect")}`);
