// O apelido gravado que o `Hello` não leva, e a recusa de gravar um apelido
// (pendência 58, item 55; lote C55 da F5).
//
// **O apelido gravado de antes.** O perfil contava caracteres até o lote C55,
// então um apelido de 20 letras `á` (40 bytes, e o fio leva 32) pode estar
// gravado nesta máquina. O `connect` voltava `Unreachable`, a entrada dizia
// NADA RESPONDEU NESSE ENDEREÇO com o servidor de pé, e encurtar o apelido no
// perfil não adiantava: a tentativa seguinte levava o de antes, guardado em
// `ultimoApelido`. Agora a FFI o recusa com nome (`ApelidoGrandeDemais`), o
// duble responde como ela, a entrada diz o teto e manda ao perfil, e a
// tentativa seguinte lê o apelido encurtado e entra.
//
// **A recusa de gravar.** O `catch` de `guardarOApelido` escrevia a frase e
// reabria o perfil, que a escondia e punha de volta o apelido gravado: a
// recusa sumia, e o que se escreveu também.
//
// Cada caso roda por conta própria, como em `tetos-dos-campos.js`.

const TETO = (await invoke("limites_dos_campos"))?.apelido;
exigir(typeof TETO === "number", "o duble não respondeu `limites_dos_campos`");
let comecoDoCaso = 0;
const mandou = (cmd) =>
  window.__SEELE_HISTORICO.slice(comecoDoCaso).filter(([c]) => c === cmd).map(([, a]) => a);
const texto = (id) => document.getElementById(id)?.textContent ?? "";
const caso = async (nome, corpo) => {
  comecoDoCaso = window.__SEELE_HISTORICO.length;
  try {
    await corpo();
    relatar(`${nome}: ok`);
  } catch (erro) {
    if (!(erro && erro.reprovado)) relatar(`REPROVADO: ${nome}: estourou: ${erro}`);
  }
};

// ---------------------------------------- a recusa de gravar fica no perfil
await caso("a recusa de gravar o apelido", async () => {
  SEELE_RESPOSTAS.escolher_apelido_local = { __recusa: "NaoGravei" };
  try {
    document.getElementById("boot-perfil").click();
    exigir(await ate(() => visivel("perfil") === "VISIVEL", 900), "o perfil não abriu na entrada");
    document.getElementById("perfil-apelido").value = "novo";
    document.getElementById("perfil-salvar").click();
    await ate(() => mandou("escolher_apelido_local").length > 0, 600);
    await espera(200);
    exigir(visivel("perfil-erro") === "VISIVEL" && texto("perfil-erro").includes(FRASES.NaoGravei.split("\n")[0]),
      `a recusa de gravar o apelido sumiu do perfil: #perfil-erro está ${visivel("perfil-erro")}, «${texto("perfil-erro")}»`);
    exigir(document.getElementById("perfil-apelido").value === "novo",
      `o apelido que se escreveu sumiu do campo com a recusa: ele diz «${document.getElementById("perfil-apelido").value}»`);
  } finally {
    delete SEELE_RESPOSTAS.escolher_apelido_local;
    document.getElementById("perfil-cancelar").click();
    await ate(() => visivel("perfil") !== "VISIVEL", 300);
  }
});

// ------------------------------------- o apelido gravado que o Hello não leva
await caso("o apelido gravado grande demais", async () => {
  const ALVO = "203.0.113.9:8383";
  const grande = "á".repeat(Math.floor(TETO / 2) + 4);
  window.__SEELE_APELIDO = grande;
  await conectar(ALVO);
  await ate(() => visivel("boot-erro") === "VISIVEL", 600);
  const primeira = mandou("connect").at(-1)?.nickname ?? "";
  exigir(primeira === grande, `a primeira tentativa não levou o apelido gravado: «${primeira}»`);
  exigir(visivel("boot-erro") === "VISIVEL" && texto("boot-erro").includes(`${TETO} BYTES`) &&
      /PERFIL/i.test(texto("boot-erro")),
    `a entrada não diz o teto do apelido nem manda ao perfil: «${texto("boot-erro")}»`);

  document.getElementById("boot-perfil").click();
  exigir(await ate(() => visivel("perfil") === "VISIVEL", 900), "o perfil não abriu depois da recusa");
  document.getElementById("perfil-apelido").value = "rafa";
  document.getElementById("perfil-salvar").click();
  exigir(await ate(() => window.__SEELE_APELIDO === "rafa", 600), "o apelido encurtado não foi gravado");
  await ate(() => visivel("perfil") !== "VISIVEL", 300);

  await conectar(ALVO);
  const segunda = mandou("connect").at(-1)?.nickname ?? "";
  exigir(segunda === "rafa",
    `depois de encurtar o apelido no perfil, a tentativa seguinte levou ${bytesDe(segunda)} bytes («${segunda}»): o apelido recusado ficou guardado`);
  exigir(await ate(() => visivel("tela-boot") !== "VISIVEL", 900),
    `a tentativa seguinte, com o apelido encurtado, não entrou: a entrada diz «${texto("boot-erro")}»`);
});
