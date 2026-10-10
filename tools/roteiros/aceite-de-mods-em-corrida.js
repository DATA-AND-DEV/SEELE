// Duas recusas por MODs em corrida — ADR 0045, lote W3 da F5, rodada 2.
//
// O que o ADR 0045 existe para não deixar acontecer é a pessoa **ler a lista de
// um pedido e aceitar o conjunto de outro**. `abrirAceiteDeMods` tinha um jeito
// estreito de fazer isso: ela grava a pergunta em `aceitePendente` e então
// espera o aceite anterior (`aceite_de_mods`) antes de desenhar. Se uma segunda
// recusa por MODs chegasse dentro dessa espera, a segunda abria a sua pergunta
// e desenhava a lista dela; a primeira, a velha, terminava depois e **repovoava
// a lista por cima**, com o servidor dela no cabeçalho — enquanto
// `aceitePendente` já era a da segunda. A pessoa lia um pedido, e ACEITAR
// gravava o sim do outro.
//
// Só a latência de um `invoke` local separa as duas, e é por isso que este
// roteiro a alarga: a leitura do aceite anterior da primeira fica pendente até
// a segunda ter aberto.
//
// A cena:
//   1. A primeira recusa (servidor A, o MOD `seele/placar`) abre, e a leitura do
//      aceite anterior dela fica pendente.
//   2. A segunda recusa (servidor B, o MOD `ponte/retransmissor`) abre direto,
//      e a pessoa tem diante de si a lista de B.
//   3. A leitura de A responde. O diálogo tem de continuar com a lista de B, no
//      servidor B, e a abertura de A vai só ao `seele.log`.
//   4. ACEITAR grava o sim de B, com o conjunto de B, e a volta vai a B.
//
// As reversões de cada exigência estão no relatório do lote W3
// (`docs/superpowers/sdd/2026-10-08-0.16/F5/lote-w3-report.md`).

const ALVO_A = "203.0.113.9:8383";
const ALVO_B = "198.51.100.4:8383";
const CONVITE_A = "convite-de-a";
const CONVITE_B = "convite-de-b";
const CONJUNTO_A = "aaaa0000aaaa0000";
const CONJUNTO_B = "bbbb1111bbbb1111";
const MOD_A = {
  id: "seele/placar",
  version: "1.4.0",
  hash: "aa11bb22cc33dd44ee55ff6677889900aa11bb22cc33dd44ee55ff6677889900",
  repo: "https://example.invalid/placar",
  reach: ["dom"],
  no_servidor: false,
};
const MOD_B = {
  id: "ponte/retransmissor",
  version: "0.8.2",
  hash: "bb22cc33dd44ee55ff6677889900aa11bb22cc33dd44ee55ff6677889900aa11",
  repo: "https://example.invalid/ponte",
  reach: ["rede de saída"],
  no_servidor: true,
};
const anuncio = (conjunto, mods) => ({ ModsNaoAceitos: { conjunto, mods } });
// Sem aspas duplas: o aparelho lê o relato até a primeira, e uma frase de
// reprovação com `"` dentro sai cortada ali.
const lista = (textos) => `[${textos.join(" · ")}]`;
const registros = () =>
  window.__SEELE_HISTORICO.filter(([cmd, args]) => cmd === "registrar_da_janela" && args.onde === "aceite-de-mods");
const desenhados = () => [...document.querySelectorAll("#mods-lista .mods-id")].map((el) => el.textContent);
const alvoNoCabecalho = () => document.getElementById("mods-alvo").textContent;

// ---- 1. A primeira recusa: a leitura do aceite anterior dela fica pendente.
let soltar;
SEELE_RESPOSTAS.aceite_de_mods = new Promise((resolver) => { soltar = resolver; });
SEELE_RESPOSTAS.connect = { __recusa: anuncio(CONJUNTO_A, [MOD_A]) };
await conectar(ALVO_A, "eu", CONVITE_A);
await espera(100);
exigir(visivel("mods-aceite") === "escondida",
  "a abertura da primeira pergunta não esperou o aceite anterior, e esta cena não mede nada");

// ---- 2. A segunda recusa, a outro servidor, abre direto.
SEELE_RESPOSTAS.aceite_de_mods = null;
SEELE_RESPOSTAS.connect = { __recusa: anuncio(CONJUNTO_B, [MOD_B]) };
await conectar(ALVO_B, "eu", CONVITE_B);
await espera(200);
exigir(visivel("mods-aceite") === "VISIVEL" && lista(desenhados()) === lista([MOD_B.id]) && alvoNoCabecalho() === ALVO_B,
  "a segunda recusa por MODs não abriu a pergunta com a lista de B, e esta cena não mede nada " +
    `(diálogo=${visivel("mods-aceite")}, itens=${lista(desenhados())}, servidor=${alvoNoCabecalho()})`);

// ---- 3. A leitura de A responde, depois de B ter aberto.
window.__SEELE_HISTORICO.length = 0;
soltar(null);
await espera(300);

const itens = desenhados();
const aceitando = aceitePendente === null ? [] : aceitePendente.mods.map((mod) => mod.id);
relatar(`depois da abertura velha: itens ${lista(itens)}, servidor ${alvoNoCabecalho()}, o ACEITAR responde por ${lista(aceitando)}`);
exigir(lista(itens) === lista(aceitando),
  `o diálogo mostra ${lista(itens)} e o ACEITAR responde por ${lista(aceitando)}: ` +
    "a pessoa lê a lista de um pedido e aceita o conjunto do outro");
exigir(lista(itens) === lista([MOD_B.id]) && alvoNoCabecalho() === ALVO_B,
  `a abertura velha, a de ${ALVO_A}, repovoou o diálogo por cima da nova: ` +
    `mostra ${lista(itens)} em ${alvoNoCabecalho()}, e a pergunta da tela é a de ${MOD_B.id} em ${ALVO_B}`);
const dos_registros = registros();
relatar(`registros no seele.log: ${dos_registros.length}`);
exigir(dos_registros.length === 1 && dos_registros[0][1].nivel === "aviso" && dos_registros[0][1].oQue.includes(ALVO_A),
  "a abertura que perdeu a vez não foi ao seele.log com o alvo e o nível aviso " +
    `(registros de «aceite-de-mods»: ${dos_registros.map(([, a]) => `${a.nivel}: ${a.oQue}`).join(" ¦ ") || "nenhum"})`);

// ---- 4. ACEITAR grava o sim de B, e a volta vai a B.
SEELE_RESPOSTAS.mods_instalados = [{ id: MOD_B.id, hash: MOD_B.hash }];
SEELE_RESPOSTAS.connect = { snapshot: SEELE_QUADRO, veredito: null };
window.__SEELE_CHAMADAS.length = 0;
delete window.__SEELE_ARGS.aceitar_mods;
document.getElementById("mods-aceitar").click();
await ate(() => window.__SEELE_CHAMADAS.includes("connect"), 1500);
await espera(100);
const gravado = window.__SEELE_ARGS.aceitar_mods;
const volta = window.__SEELE_ARGS.connect;
relatar(`o sim: alvo=${gravado?.alvo} conjunto=${gravado?.conjunto}; a volta: servidor=${volta?.server}`);
exigir(gravado !== undefined && gravado.alvo === ALVO_B && gravado.conjunto === CONJUNTO_B,
  `ACEITAR gravou o sim de ${gravado?.alvo} (${gravado?.conjunto}), e a lista que a pessoa leu era a de ${ALVO_B} (${CONJUNTO_B})`);
exigir(volta.server === ALVO_B,
  `a volta foi a ${volta.server}, e a pergunta que a pessoa respondeu era a de ${ALVO_B}`);
