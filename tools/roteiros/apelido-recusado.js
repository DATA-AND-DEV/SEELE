// A recusa chega como `{ Refused: { reason } }`, que é a forma do fio.
const recusa = { Refused: { reason: "NicknameTaken" } };
relatar("sem o apelido: " + fraseDeErro(recusa).split("\n")[0]);
relatar("com o apelido: " + fraseDeErro(recusa, "pessoa-a3f1").split("\n")[0]);

// O caso `apelido-tomado` (lote W da F5). O servidor recusa o apelido antes de
// gastar o convite, e a frase manda escolher outro e entrar de novo: a
// tentativa seguinte ao mesmo servidor leva o mesmo convite e o apelido novo,
// sem a pessoa colar o link outra vez. Uma tentativa a outro servidor não leva
// convite nenhum, e esquece o que ficou.
const ALVO = "203.0.113.9:8383";
const CONVITE = "convite-do-roteiro";
SEELE_RESPOSTAS.connect = { __recusa: recusa };
await conectar(ALVO, null, CONVITE);
await espera(200);
relatar("recusado: " + document.getElementById("boot-erro").textContent.split("\n")[0]);

// O que o SALVAR do perfil grava.
await invoke("escolher_apelido_local", { apelido: "rafa-2" });
SEELE_RESPOSTAS.connect = { snapshot: SEELE_QUADRO, veredito: null };
await conectar(ALVO);
const seguinte = window.__SEELE_ARGS.connect;
relatar(`a tentativa seguinte: apelido=${seguinte.nickname} convite=${seguinte.joinSecret ?? "nenhum"}`);
exigir(seguinte.joinSecret === CONVITE,
  "a tentativa seguinte ao mesmo servidor saiu sem o convite que o apelido tomado não gastou: CREDENCIAL RECUSADA sobre um convite válido");
exigir(seguinte.nickname === "rafa-2",
  "a tentativa seguinte mandou o apelido recusado, e não o que o perfil acabou de gravar");

SEELE_RESPOSTAS.connect = { __recusa: recusa };
await conectar(ALVO, null, CONVITE);
await conectar("198.51.100.4:8383");
const outro = window.__SEELE_ARGS.connect;
relatar(`a outro servidor: convite=${outro.joinSecret ?? "nenhum"}`);
exigir(outro.joinSecret == null, "uma tentativa a outro servidor levou o convite deste");
await conectar(ALVO);
const volta = window.__SEELE_ARGS.connect;
relatar(`de volta, depois do outro: convite=${volta.joinSecret ?? "nenhum"}`);
exigir(volta.joinSecret == null, "trocar de servidor não esqueceu o convite que o apelido tomado deixou");
