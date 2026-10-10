// O aceite de MODs que falha por dentro — ADR 0045, lote W3 da F5.
//
// `aceite-de-mods.js` prova o aceite que dá certo. Este prova o que a tela faz
// quando `aceitarOsMods` **rejeita** por um caminho que nenhum `try` dele pega,
// e que antes era um `console.warn`, o ACEITAR apagado e nenhuma palavra na
// tela: «o produto sabe e não conta».
//
// **O que rejeita de verdade**, medido: nenhum `invoke` do aceite. As duas
// leituras de `mods_instalados`, a obtenção, o `aceitar_mods` e o
// `esquecer_aceite_de_mods` estão dentro de um `try` que escreve uma frase, e
// a volta é do `conectar`. O que escapa é a **resposta de forma errada**: uma
// lista de instalados que não é lista estoura em `.some`, fora de qualquer
// `try`. O produto não a manda hoje (o comando devolve um `Vec`); o duble a
// mandava, e foi assim que `aceite-de-mods.js` ficou semanas dizendo
// `alvo=undefined` sem que a tela dissesse nada. É essa resposta que este
// roteiro simula, porque é a que rejeita.
//
// Duas cenas:
//   1. A pergunta que falhou **fala**: a frase no diálogo, com o alvo e o
//      motivo; o ACEITAR volta; o motivo vai ao `seele.log` por
//      `registrar_da_janela`, sem o convite; e, com a resposta certa, o mesmo
//      ACEITAR grava o sim e entra levando o convite que a falha não comeu.
//   2. A pergunta que largou **não fala**: quem desistiu enquanto o aceite
//      andava, e já está diante de outra pergunta, não lê o erro da antiga na
//      tela nova. O registro continua a ser escrito.
//
// As reversões de cada exigência estão no relatório do lote W3
// (`docs/superpowers/sdd/2026-10-08-0.16/F5/lote-w3-report.md`).

const ALVO = "203.0.113.9:8383";
const CONVITE = "convite-do-roteiro";
const ANUNCIO = {
  ModsNaoAceitos: {
    conjunto: "c0ffee1234567890",
    mods: [
      {
        id: "seele/placar",
        version: "1.4.0",
        hash: "aa11bb22cc33dd44ee55ff6677889900aa11bb22cc33dd44ee55ff6677889900",
        repo: "https://example.invalid/placar",
        reach: ["dom"],
        no_servidor: false,
      },
    ],
  },
};
const EXATOS = ANUNCIO.ModsNaoAceitos.mods.map(({ id, hash }) => ({ id, hash }));
const registros = () =>
  window.__SEELE_HISTORICO.filter(([cmd, args]) => cmd === "registrar_da_janela" && args.onde === "aceite-de-mods");

// -------------------------------------------------- 1. a pergunta que falhou
SEELE_RESPOSTAS.connect = { __recusa: ANUNCIO };
SEELE_RESPOSTAS.aceite_de_mods = null;
// **A resposta que o produto nunca manda**, e que faz `aceitarOsMods` rejeitar.
SEELE_RESPOSTAS.mods_instalados = null;
await conectar(ALVO, "eu", CONVITE);
await espera(300);
exigir(visivel("mods-aceite") === "VISIVEL",
  "a recusa por MODs não abriu o diálogo, e este roteiro não mede nada sem ele");

window.__SEELE_CHAMADAS.length = 0;
window.__SEELE_HISTORICO.length = 0;
document.getElementById("mods-aceitar").click();
await ate(() => visivel("mods-erro") === "VISIVEL", 1000);
await espera(100);

const erro = document.getElementById("mods-erro");
const sequencia = window.__SEELE_CHAMADAS.join(" → ") || "nenhuma";
relatar(`depois de ACEITAR com a resposta errada: ${sequencia}`);
relatar(`mods-erro: ${visivel("mods-erro")} «${erro.textContent.slice(0, 70)}…»`);
exigir(visivel("mods-erro") === "VISIVEL" && erro.textContent.includes(ALVO) && erro.textContent.includes("Tente de novo"),
  "ACEITAR rejeitou por dentro e a tela não disse o que falhou, com o alvo e o caminho de volta " +
    `(mods-erro=${visivel("mods-erro")}, a sequência foi: ${sequencia})`);
relatar(`ACEITAR: ${document.getElementById("mods-aceitar").disabled ? "apagado" : "aceso"}, diálogo ${visivel("mods-aceite")}`);
exigir(!document.getElementById("mods-aceitar").disabled && visivel("mods-aceite") === "VISIVEL",
  "ACEITAR rejeitou por dentro e o botão ACEITAR ficou apagado: a pessoa não tem como tentar de novo");

const dos_registros = registros();
relatar(`registros no seele.log: ${dos_registros.length}`);
exigir(dos_registros.length === 1 && dos_registros[0][1].nivel === "aviso"
    && dos_registros[0][1].oQue.includes(ALVO) && dos_registros[0][1].oQue.includes("TypeError"),
  "ACEITAR rejeitou por dentro e o motivo não foi ao seele.log com o alvo, o erro e o nível aviso " +
    `(registros de «aceite-de-mods»: ${JSON.stringify(dos_registros.map(([, a]) => a))})`);
exigir(!JSON.stringify(window.__SEELE_HISTORICO.filter(([cmd]) => cmd === "registrar_da_janela")).includes(CONVITE),
  "o registro do aceite que falhou levou o convite da tentativa, que só pode ir ao connect");

// E o mesmo ACEITAR, com a resposta certa, grava o sim e entra — levando o
// convite que a falha não comeu.
SEELE_RESPOSTAS.mods_instalados = EXATOS;
SEELE_RESPOSTAS.connect = { snapshot: SEELE_QUADRO, veredito: null };
window.__SEELE_CHAMADAS.length = 0;
delete window.__SEELE_ARGS.aceitar_mods;
document.getElementById("mods-aceitar").click();
await ate(() => window.__SEELE_CHAMADAS.includes("connect"), 1500);
await espera(100);
const volta = window.__SEELE_ARGS.connect;
relatar(`a segunda vez: ${window.__SEELE_CHAMADAS.slice(0, 4).join(" → ")}; convite=${volta?.joinSecret ?? "nenhum"}; ` +
  `diálogo ${visivel("mods-aceite")}`);
exigir(window.__SEELE_ARGS.aceitar_mods?.alvo === ALVO && window.__SEELE_CHAMADAS.includes("connect"),
  "depois da falha, o mesmo ACEITAR com a resposta certa não gravou o sim e entrou " +
    `(a sequência foi: ${window.__SEELE_CHAMADAS.join(" → ") || "nenhuma"})`);
exigir(volta.joinSecret === CONVITE,
  `a falha comeu a credencial da tentativa: a volta saiu com joinSecret=${JSON.stringify(volta.joinSecret ?? null)}`);
exigir(visivel("mods-aceite") === "escondida" && visivel("mods-erro") === "escondida",
  "a segunda tentativa entrou, e o diálogo ou a frase da falha continuou na tela " +
    `(diálogo=${visivel("mods-aceite")}, mods-erro=${visivel("mods-erro")})`);

// ---------------------------------------------- 2. a pergunta que largou
// O aceite da primeira pergunta espera a leitura dos instalados, que fica
// pendente; a pessoa desiste, vem outra pergunta; e só então a leitura chega,
// com a resposta errada.
let soltar;
SEELE_RESPOSTAS.mods_instalados = new Promise((resolver) => { soltar = resolver; });
SEELE_RESPOSTAS.connect = { __recusa: ANUNCIO };
await conectar(ALVO, "eu", CONVITE);
await espera(200);
document.getElementById("mods-aceitar").click();
await espera(100);
document.getElementById("mods-recusar").click();
await espera(100);
await conectar(ALVO, "eu", CONVITE);
await espera(200);
exigir(visivel("mods-aceite") === "VISIVEL",
  "a segunda recusa por MODs não abriu a pergunta nova, e esta cena não mede nada");

window.__SEELE_HISTORICO.length = 0;
soltar(null);
await espera(300);
relatar(`na pergunta nova: mods-erro ${visivel("mods-erro")}, ACEITAR ${document.getElementById("mods-aceitar").disabled ? "apagado" : "aceso"}`);
exigir(visivel("mods-erro") === "escondida",
  "o erro de uma pergunta que a pessoa largou apareceu na pergunta nova: " +
    `«${document.getElementById("mods-erro").textContent}»`);
exigir(registros().length === 1,
  `o aceite de uma pergunta que largou rejeitou e não foi ao seele.log (${registros().length} registros em vez de 1)`);
