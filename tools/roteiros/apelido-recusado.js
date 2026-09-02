// A recusa chega como `{ Refused: { reason } }`, que é a forma do fio.
const recusa = { Refused: { reason: "NicknameTaken" } };
relatar("sem o apelido: " + fraseDeErro(recusa).split("\n")[0]);
relatar("com o apelido: " + fraseDeErro(recusa, "pessoa-a3f1").split("\n")[0]);
