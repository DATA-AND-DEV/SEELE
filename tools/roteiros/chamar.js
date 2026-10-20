// Quem liga pelo código vê CHAMANDO com o nome de quem chama, pode CANCELAR, e
// lê o desfecho com o nome (tarefa 28 da F7; spec de 23/09, «Do lado de quem
// liga», 5, e «Erros e frases»).
//
// O `connect` do duble fica pendente numa promessa que o roteiro resolve, e as
// etapas chegam pelo `__SEELE_EMITIR`, como a ponte as entrega. O nome é o que
// o Rust leu da agenda (`ligar::EntradaColada::Codigo { nome }`).
//
// 1. Enquanto a ligação anda, o CANCELAR está à vista; no `Tocando`, a etapa é
//    «CHAMANDO ALICE…».
// 2. O CANCELAR chama `cancelar_ligacao`; a ligação cancelada diz que foi a
//    pessoa, e o CANCELAR some.
// 3. «ALICE NÃO ATENDEU»: o desfecho com o nome de quem foi chamado.
// 4. Uma entrada num servidor, que não é ligação, não mostra CANCELAR, e a falha
//    dela é a frase de sempre.

const CODIGO = "CSM7-NBFR-CAYQ-EV4Y";
const ALVO = "@" + CODIGO;

const chamou = (cmd) => window.__SEELE_CHAMADAS.includes(cmd);
const etapa = () => document.getElementById("boot-etapa").textContent;
const erroNaEntrada = () =>
  visivel("boot-erro") === "VISIVEL" ? document.getElementById("boot-erro").textContent : "";
const cancelarAVista = () => visivel("boot-cancelar-ligacao") === "VISIVEL";
const colar = async (texto) => {
  await abrirServidores();
  document.getElementById("servidores-endereco").value = texto;
  window.__SEELE_CHAMADAS.length = 0;
  document.getElementById("servidores-forma").dispatchEvent(
    new Event("submit", { cancelable: true }),
  );
  await ate(() => chamou("connect"), 800);
};
const ligar = async () => {
  let fim;
  SEELE_RESPOSTAS.connect = new Promise((ok, falha) => {
    fim = { ok, falha };
  });
  SEELE_RESPOSTAS.ler_entrada = { Codigo: { codigo: CODIGO, alvo: ALVO, nome: "Alice" } };
  await colar(CODIGO);
  return fim;
};

// 1. Ligando: o CANCELAR à vista, e o toque com o nome.
let fim = await ligar();
exigir(chamou("connect") && window.__SEELE_ARGS.connect?.server === ALVO,
  `a ligação não foi ao connect com o alvo: ${window.__SEELE_ARGS.connect?.server}`);
exigir(cancelarAVista(), "o CANCELAR não está à vista enquanto a ligação anda");
window.__SEELE_EMITIR({ ConnectStageChanged: { stage: { Tocando: { candidato: 0, prazo_s: 45 } } } });
await ate(() => etapa().includes("CHAMANDO"), 400);
relatar(`tocando: «${etapa()}» · cancelar ${visivel("boot-cancelar-ligacao")}`);
exigir(etapa() === "CHAMANDO ALICE…", `o toque não diz quem está sendo chamado: «${etapa()}»`);

// 2. O CANCELAR.
document.getElementById("boot-cancelar-ligacao").click();
await ate(() => chamou("cancelar_ligacao"), 400);
exigir(chamou("cancelar_ligacao"), "o CANCELAR não chamou `cancelar_ligacao`");
fim.falha({ error: "Cancelada", trail: [] });
await ate(() => erroNaEntrada() !== "", 600);
relatar(`cancelada: «${erroNaEntrada()}» · cancelar ${visivel("boot-cancelar-ligacao")}`);
exigir(erroNaEntrada() === FRASES.Cancelada,
  `a ligação cancelada não diz que foi a pessoa: «${erroNaEntrada()}»`);
exigir(!cancelarAVista(), "o CANCELAR ficou à vista depois de a ligação acabar");

// 3. Ninguém atendeu: com o nome.
fim = await ligar();
fim.falha({ error: { Refused: { reason: "NinguemAtendeu" } }, trail: [] });
await ate(() => erroNaEntrada().includes("ATENDEU"), 600);
relatar(`não atendeu: «${erroNaEntrada()}»`);
exigir(erroNaEntrada() === "ALICE NÃO ATENDEU",
  `o «não atendeu» não diz quem foi chamado: «${erroNaEntrada()}»`);
exigir(!cancelarAVista(), "o CANCELAR ficou à vista depois de ninguém atender");

// 4. Um servidor, que não é ligação.
delete SEELE_RESPOSTAS.ler_entrada;
SEELE_RESPOSTAS.connect = { __recusa: { error: "Unreachable", trail: [] } };
await colar("casa.exemplo:8383");
await ate(() => erroNaEntrada() !== "", 600);
relatar(`servidor: «${erroNaEntrada()}» · cancelar ${visivel("boot-cancelar-ligacao")}`);
exigir(erroNaEntrada() === fraseDeErro("Unreachable"),
  `a falha de um servidor ganhou frase de ligação: «${erroNaEntrada()}»`);
exigir(!cancelarAVista(), "uma entrada num servidor mostrou o CANCELAR de ligação");
