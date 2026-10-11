// A recusa «apagada pela moderação», vista na tela: a frase, e só DESISTIR.
//
// A moderação apagou a mensagem enquanto quem a escreveu estava fora, e o
// servidor recusa a repetição com `MessageRefusal::Apagada`. Tentar de novo
// nunca passa, então a pendente fica só com DESISTIR, que devolve o texto ao
// campo e a fecha. Uma recusa de armazenamento ao lado é o controle: ela
// continua com os dois botões, e é isso que prova que o roteiro olha para o
// lugar certo.
await entrarNaSessao();

const pendente = (id, corpo, motivo) => ({
  ...M(id, 1, "rafa", 1756000700 + id, corpo, true),
  estado: { Recusada: motivo },
  client_message_id: String(1000 + id),
});
const APAGADA = "isto a moderação apagou";
const CONTROLE = "isto o servidor não gravou";
SEELE_RESPOSTAS.messages = [
  ...SEELE_RESPOSTAS.messages,
  pendente(6, APAGADA, "Apagada"),
  pendente(7, CONTROLE, "StorageFailed"),
];
SEELE_QUADRO.messages_revision = (SEELE_QUADRO.messages_revision ?? 0) + 1;

// O selo de envio da mensagem com este corpo.
const selo = (corpo) => [...document.querySelectorAll(".envio")]
  .find((s) => s.parentElement?.textContent.includes(corpo));
const frase = (corpo) => selo(corpo)?.querySelector(".envio-frase")?.textContent ?? "";
const botao = (corpo, acao) => selo(corpo)?.querySelector(`[data-acao="${acao}"]`) ?? null;

exigir(await ate(() => selo(APAGADA) && selo(CONTROLE), 2000),
  "as duas pendentes não apareceram na conversa, e o roteiro mediu outra coisa");

// O controle primeiro: sem ele, «não há TENTAR DE NOVO» passaria também com o
// seletor errado.
exigir(botao(CONTROLE, "reenviar") && botao(CONTROLE, "descartar"),
  "a recusa de armazenamento perdeu um dos dois botões, e o roteiro não mede nada");

exigir(frase(APAGADA) === "não foi gravada: apagada pela moderação",
  `a apagada diz «${frase(APAGADA)}»`);
exigir(botao(APAGADA, "reenviar") === null,
  "a apagada oferece TENTAR DE NOVO, e repetir recebe a mesma recusa para sempre");
exigir(botao(APAGADA, "descartar") !== null,
  "a apagada ficou sem DESISTIR, e a pendente não tem como fechar");
relatar("apagada: " + frase(APAGADA) + " | botões: " +
  [...selo(APAGADA).querySelectorAll("button")].map((b) => b.textContent).join(", "));
