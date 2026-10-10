// A pergunta dos MODs que não abre — ADR 0045, lote W3 da F5, rodada 1.
//
// `aceite-de-mods.js` prova o aceite que dá certo, e `aceite-de-mods-que-falha.js`
// o clique em ACEITAR que rejeita por dentro. Este prova o outro `.catch` da
// mesma classe: o da abertura. `levarParaOAceiteDeMods` abria a pergunta com
// `.catch(console.warn)` e devolvia `true`, que `conectar` lê como «a pergunta
// tratou»: se a abertura rejeitasse, a pessoa ficava **sem diálogo e sem linha
// vermelha**, e a credencial da tentativa — que `abrirAceiteDeMods` grava antes
// de rejeitar — ficava na memória da janela com uma pergunta que ninguém vê e
// nenhuma porta fecha (nem RECUSAR, nem Escape, nem clicar fora).
//
// **O que rejeita de verdade**: nenhum `invoke` da abertura (o único, o aceite
// anterior, está num `try`). Rejeita a **lista malformada** — `mods.map(...)` e
// `linhaDeMod` estouram numa lista que não é lista, ou que traz um `hash` que
// não é texto. O produto não a manda hoje (o anúncio vem decodificado do Rust),
// e é por isso que o roteiro a simula.
//
// Quatro cenas, e a tentativa mais nova é a única que fala (lote W2):
//   1. A pergunta que não abriu **fala**: o diálogo fica fechado, a linha
//      vermelha da entrada diz a recusa («entre de novo neste servidor»), o
//      motivo vai ao `seele.log` sem o convite, e a pergunta com a credencial
//      não fica na memória.
//   2. Se outra tentativa começou antes de a falha chegar, a tela **não** fala;
//      o registro e a limpeza continuam.
//   3. Se outra tentativa já abriu a própria pergunta, a falha da antiga **não a
//      fecha**.
//   4. Se a frase da recusa também não se monta (a lista nem é lista), a linha
//      vermelha diz a falha de abrir, e não fica calada.
//
// As reversões de cada exigência estão no relatório do lote W3
// (`docs/superpowers/sdd/2026-10-08-0.16/F5/lote-w3-report.md`).

const ALVO = "203.0.113.9:8383";
const CONVITE = "convite-do-roteiro";
const MOD = {
  id: "seele/placar",
  version: "1.4.0",
  hash: "aa11bb22cc33dd44ee55ff6677889900aa11bb22cc33dd44ee55ff6677889900",
  repo: "https://example.invalid/placar",
  reach: ["dom"],
  no_servidor: false,
};
const anuncio = (mods) => ({ ModsNaoAceitos: { conjunto: "c0ffee1234567890", mods } });
// Um `hash` que não é texto: `linhaDeMod` estoura nele, e a frase da recusa
// (`id`, `versão`, repositório, alcance) não toca o hash e se monta.
const LISTA_QUEBRADA = anuncio([{ ...MOD, hash: 12345 }]);
const registros = () =>
  window.__SEELE_HISTORICO.filter(([cmd, args]) => cmd === "registrar_da_janela" && args.onde === "aceite-de-mods");
// Sem aspas duplas: o aparelho lê o relato até a primeira, e uma frase de
// reprovação com `"` dentro sai cortada ali.
const resumo = (linhas) => linhas.map(([, a]) => `${a.nivel}: ${a.oQue}`).join(" ¦ ") || "nenhum";
const pendente = () =>
  aceitePendente === null ? "nenhuma" : `uma pergunta de ${aceitePendente.alvo}, com o diálogo ${visivel("mods-aceite")}`;
const linhaVermelha = () => document.getElementById("boot-erro").textContent.split("\n")[0];

// ---------------------------------------- 1. a pergunta que não abriu fala
SEELE_RESPOSTAS.aceite_de_mods = null;
SEELE_RESPOSTAS.connect = { __recusa: LISTA_QUEBRADA };
window.__SEELE_HISTORICO.length = 0;
await conectar(ALVO, "eu", CONVITE);
await espera(300);

relatar(`1. diálogo ${visivel("mods-aceite")}, linha vermelha ${visivel("boot-erro")} «${linhaVermelha()}», pergunta pendente: ${pendente()}`);
exigir(visivel("mods-aceite") === "escondida",
  "a abertura da pergunta rejeitou e o diálogo ficou aberto, meio desenhado");
exigir(visivel("boot-erro") === "VISIVEL" && document.getElementById("boot-erro").textContent.includes("entre de novo"),
  "a pergunta dos MODs não abriu e a entrada não diz nada: sem diálogo e sem linha vermelha com a recusa " +
    `(linha vermelha=${visivel("boot-erro")}, o registro tem ${registros().length} linha(s))`);
exigir(aceitePendente === null,
  `a pergunta que não abriu ficou na memória com a credencial da tentativa, sem diálogo que a feche: ${pendente()}`);
const dos_registros = registros();
exigir(dos_registros.length === 1 && dos_registros[0][1].nivel === "aviso"
    && dos_registros[0][1].oQue.includes(ALVO) && dos_registros[0][1].oQue.includes("TypeError"),
  "a pergunta dos MODs não abriu e o motivo não foi ao seele.log com o alvo, o erro e o nível aviso " +
    `(registros de «aceite-de-mods»: ${resumo(dos_registros)})`);
exigir(!JSON.stringify(window.__SEELE_HISTORICO.filter(([cmd]) => cmd === "registrar_da_janela")).includes(CONVITE),
  "o registro da pergunta que não abriu levou o convite da tentativa, que só pode ir ao connect");

// ------------------------- 2. a tentativa que outra substituiu não fala na tela
// A abertura da pergunta espera o aceite anterior, que fica pendente; outra
// tentativa começa e entra; e só então a abertura falha.
let soltar;
SEELE_RESPOSTAS.aceite_de_mods = new Promise((resolver) => { soltar = resolver; });
SEELE_RESPOSTAS.connect = { __recusa: LISTA_QUEBRADA };
await conectar(ALVO, "eu", CONVITE);
await espera(100);
SEELE_RESPOSTAS.connect = { snapshot: SEELE_QUADRO, veredito: null };
await conectar(ALVO, "eu");
await espera(200);
exigir(visivel("boot-erro") === "escondida",
  "a tentativa nova começou e a linha vermelha da antiga ficou na tela, e esta cena não mede nada");

window.__SEELE_HISTORICO.length = 0;
soltar(null);
await espera(300);
relatar(`2. linha vermelha ${visivel("boot-erro")}, pergunta pendente: ${pendente()}, registros ${registros().length}`);
exigir(visivel("boot-erro") === "escondida",
  "a pergunta de uma tentativa que outra substituiu não abriu e a falha apareceu na linha vermelha da entrada: " +
    `«${linhaVermelha()}»`);
exigir(aceitePendente === null,
  `a pergunta de uma tentativa que outra substituiu não abriu e ficou na memória com a credencial: ${pendente()}`);
exigir(registros().length === 1,
  `a pergunta de uma tentativa que outra substituiu não abriu e não foi ao seele.log (${registros().length} registros em vez de 1)`);

// ------------- 3. a falha da pergunta antiga não fecha a pergunta nova
// A mesma espera; e a tentativa nova, em vez de entrar, abre a própria pergunta.
SEELE_RESPOSTAS.aceite_de_mods = new Promise((resolver) => { soltar = resolver; });
SEELE_RESPOSTAS.connect = { __recusa: LISTA_QUEBRADA };
await conectar(ALVO, "eu", CONVITE);
await espera(100);
SEELE_RESPOSTAS.aceite_de_mods = null;
SEELE_RESPOSTAS.connect = { __recusa: anuncio([MOD]) };
await conectar(ALVO, "eu", CONVITE);
await espera(200);
exigir(visivel("mods-aceite") === "VISIVEL",
  "a segunda recusa por MODs não abriu a pergunta nova, e esta cena não mede nada");

window.__SEELE_HISTORICO.length = 0;
soltar(null);
await espera(300);
relatar(`3. diálogo ${visivel("mods-aceite")}, linha vermelha ${visivel("boot-erro")}, pergunta pendente: ${pendente()}`);
exigir(visivel("mods-aceite") === "VISIVEL" && aceitePendente !== null,
  "a pergunta antiga não abriu e a falha dela fechou a pergunta nova, que a pessoa estava lendo " +
    `(diálogo=${visivel("mods-aceite")}, pergunta pendente: ${pendente()})`);
exigir(visivel("boot-erro") === "escondida",
  `a falha da pergunta antiga apareceu na linha vermelha, por cima da pergunta nova: «${linhaVermelha()}»`);
document.getElementById("mods-recusar").click();
await espera(50);

// ------------- 4. a frase da recusa também não se monta: a falha diz o que houve
SEELE_RESPOSTAS.aceite_de_mods = null;
SEELE_RESPOSTAS.connect = { __recusa: anuncio("isto não é uma lista") };
await conectar(ALVO, "eu", CONVITE);
await espera(300);
relatar(`4. diálogo ${visivel("mods-aceite")}, linha vermelha ${visivel("boot-erro")} «${linhaVermelha()}», pergunta pendente: ${pendente()}`);
exigir(visivel("boot-erro") === "VISIVEL" && document.getElementById("boot-erro").textContent.includes("TypeError"),
  "a lista da recusa nem é lista, a frase dela não se montou e a entrada ficou calada, em vez de dizer a falha " +
    `(linha vermelha=${visivel("boot-erro")}: «${linhaVermelha()}»)`);
exigir(aceitePendente === null,
  `a pergunta de uma lista que nem é lista ficou na memória com a credencial: ${pendente()}`);
