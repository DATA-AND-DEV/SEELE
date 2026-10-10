// Entrar num servidor de outra linha abre o SEELE **daquela** linha — ADR 0046,
// com a emenda de 08/10/2026.
//
// É a frase inteira da decisão, e o que este roteiro prova é que ela acontece
// **antes de conectar**: a versão vem no `seele://`, e não no fio, porque um
// servidor de outra linha pode falar um protocolo que este cliente já não
// alcança — ele seria recusado com «versão incompatível» sem chegar a dizer
// uma palavra sobre si.
//
// **A resposta do `analisar_convite` é a de hoje**: o `ConviteLido` de
// `main.rs`, cujo `para_entrar` é o `versoes::ParaEntrar` que o Rust decide
// (`Aqui`, `NaoSabe`, `{ Abrir }`, `{ Baixar }`, `{ LinhaNaoOferecida }`). A
// casca pergunta antes de abrir, com a frase das conversas, e só o ABRIR chama
// `abrir_versao`, com a linha e o link. Este roteiro respondia com a forma de
// antes da emenda (`versao` e `pode_abrir_naquela_versao`), que a casca não lê
// mais: de 08/10 a 10/10/2026 ele relatou «abriu outra versão: false ·
// conectou aqui mesmo: true», o contrário do que diz provar, e ficou verde,
// sem nenhum `exigir` (lote RT da F5).

const LINK = "seele://casa.exemplo:8383/?fp=" + "a".repeat(64) + "&v=0.15";

const mandar = async () => {
  document.getElementById("botao-conectar").click();
  await espera(150);
  document.getElementById("servidores-endereco").value = LINK;
  window.__SEELE_CHAMADAS.length = 0;
  delete window.__SEELE_ARGS.abrir_versao;
  document.getElementById("servidores-forma").dispatchEvent(
    new Event("submit", { cancelable: true }),
  );
  await espera(300);
};
const chamou = (cmd) => window.__SEELE_CHAMADAS.includes(cmd);

// O que o Rust responde ao ler aquele link: o servidor é da linha 0.15, e o
// app dela está guardado aqui.
SEELE_RESPOSTAS.analisar_convite = {
  alvo: "casa.exemplo:8383",
  token: null,
  versao: "0.15",
  para_entrar: {
    Abrir: { linha: "0.15", servidor: "0.15", versao: "0.15.2", perde_conversas: false },
  },
};
await mandar();

// Antes do ABRIR: a pergunta, e nada aberto nem conectado.
const pergunta = document.getElementById("versoes-confirmar-frase")?.textContent ?? "";
relatar(`a pergunta: ${visivel("versoes-confirmar")} «${pergunta}»`);
exigir(visivel("versoes-confirmar") === "VISIVEL" && pergunta.includes("Servidor 0.15"),
  `o link de outra linha não perguntou antes de abrir: versoes-confirmar ${visivel("versoes-confirmar")}, «${pergunta}»`);
relatar(`antes do ABRIR: abriu outra versão ${chamou("abrir_versao")}, conectou ${chamou("connect")}`);
exigir(!chamou("abrir_versao") && !chamou("connect"),
  `antes da resposta da pessoa já houve ${window.__SEELE_CHAMADAS.join(", ")}`);

document.getElementById("versoes-confirmar-sim").click();
await espera(300);

const pedido = window.__SEELE_ARGS.abrir_versao ?? {};
relatar(`abriu outra versão: ${chamou("abrir_versao")}`);
relatar(`linha pedida: ${pedido.linha}`);
relatar(`levou o link inteiro: ${pedido.link === LINK}`);
relatar(`pediu para hospedar: ${pedido.hospedar}`);
// A metade que importa tanto quanto: **não** conectou nesta versão.
relatar(`conectou aqui mesmo: ${chamou("connect")}`);
relatar(`o estado: «${document.getElementById("versoes-confirmar-estado")?.textContent ?? ""}»`);
exigir(chamou("abrir_versao") && pedido.linha === "0.15",
  `o ABRIR não abriu o SEELE da linha do servidor: abrir_versao ${chamou("abrir_versao")}, linha ${pedido.linha}`);
exigir(pedido.link === LINK,
  `o SEELE da outra linha não recebeu o link inteiro, e não teria como ir ao servidor: ${pedido.link}`);
exigir(pedido.hospedar === false,
  `entrar num servidor de outra linha pediu para hospedar: ${pedido.hospedar}`);
exigir(!chamou("connect"),
  "depois de abrir o SEELE da outra linha, esta janela também conectou");

// E o outro lado: um servidor da linha deste app não desvia nada.
SEELE_RESPOSTAS.analisar_convite = {
  alvo: "casa.exemplo:8383",
  token: null,
  versao: "1.0",
  para_entrar: "Aqui",
};
await mandar();
relatar(`mesma linha, abriu outra: ${chamou("abrir_versao")}`);
relatar(`mesma linha, conectou: ${chamou("connect")}`);
exigir(!chamou("abrir_versao") && chamou("connect"),
  `um servidor da linha deste app não conectou aqui: abrir_versao ${chamou("abrir_versao")}, connect ${chamou("connect")}`);
