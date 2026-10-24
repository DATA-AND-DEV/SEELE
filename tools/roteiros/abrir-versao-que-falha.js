// Uma falha do `abrir_versao` não volta calada (medida com tela no Mac,
// 23/10/2026). Pelo `--entrar`, a orquestradora viu o ABRIR fechar o painel e
// nenhum processo nascer, sem uma linha no `seele.log`: a frase da falha morava
// só dentro do painel de servidores, e com ele fechado ninguém a lia.
//
// **Reprova** se a falha, pelo CONECTAR ou pelo `--entrar`, não for ao
// `seele.log` (`registrar_da_janela`); se a frase não ficar à vista, inclusive
// quando o painel fechou durante o ABRIR; ou se o convite do link for ao
// registro.

const CONVITE = "convite-de-uso-unico-3d8a";
const LINK = "seele://casa.exemplo:8383/?fp=" + "d".repeat(64) + "&v=0.15.1&convite=" + CONVITE;
const FALHA = { NaoIniciou: "o sistema recusou abrir o executável" };
const DECISAO = { Abrir: { linha: "0.15", servidor: "0.15", versao: "0.15.2", perde_conversas: false } };
const registros = () => window.__SEELE_HISTORICO
  .filter(([cmd]) => cmd === "registrar_da_janela")
  .map(([, args]) => JSON.stringify(args ?? {}));
const texto = (id) => document.getElementById(id)?.textContent ?? "";

SEELE_RESPOSTAS.analisar_convite = { alvo: "casa.exemplo:8383", token: CONVITE, versao: "0.15.1", para_entrar: DECISAO };
SEELE_RESPOSTAS.abrir_versao = { __recusa: FALHA };
const esperada = fraseDaVersao(FALHA);

// ---- 1. Pelo CONECTAR: o link colado no painel.
document.getElementById("botao-conectar").click();
await espera(150);
document.getElementById("servidores-endereco").value = LINK;
window.__SEELE_HISTORICO.length = 0;
document.getElementById("servidores-forma").dispatchEvent(new Event("submit", { cancelable: true }));
await ate(() => document.getElementById("versoes-confirmar")?.hidden === false, 1000);
document.getElementById("versoes-confirmar-sim").click();
await ate(() => registros().length > 0, 1000);
let ditos = registros();
relatar(`CONECTAR: registros ${ditos.length} · erro ${visivel("servidores-erro")} «${texto("servidores-erro")}»`);
exigir(ditos.some((r) => r.includes("NaoIniciou")),
  `pelo CONECTAR, a falha do abrir_versao não foi ao seele.log: ${ditos.join(" | ") || "nenhum registro"}`);
exigir(visivel("servidores") === "VISIVEL" && visivel("servidores-erro") === "VISIVEL" && texto("servidores-erro") === esperada,
  `pelo CONECTAR, a frase da falha não ficou à vista: «${texto("servidores-erro")}»`);
exigir(!ditos.some((r) => r.includes(CONVITE)), "o convite do link foi ao seele.log");

// ---- 2. Pelo `--entrar`, com o painel fechando no meio do ABRIR.
document.getElementById("servidores-fechar").click();
await espera(100);
SEELE_RESPOSTAS.abertura = { entrar: LINK, hospedar: false, nome_publico: null };
window.__SEELE_HISTORICO.length = 0;
document.getElementById("boot-erro").hidden = true;
await cumprirAAbertura();
await ate(() => document.getElementById("versoes-confirmar")?.hidden === false, 1000);
document.getElementById("versoes-confirmar-sim").click();
// Antes de a recusa voltar (ela volta numa microtarefa): o painel fecha.
fecharServidores();
await ate(() => registros().length > 0, 1000);
delete SEELE_RESPOSTAS.abertura;
ditos = registros();
relatar(`--entrar: registros ${ditos.length} · painel ${visivel("servidores")} · boot-erro ${visivel("boot-erro")} «${texto("boot-erro")}»`);
exigir(ditos.some((r) => r.includes("NaoIniciou")),
  `pelo --entrar, a falha do abrir_versao não foi ao seele.log: ${ditos.join(" | ") || "nenhum registro"}`);
exigir(visivel("boot-erro") === "VISIVEL" && texto("boot-erro") === esperada,
  `com o painel fechado, a frase da falha não apareceu na tela de entrada: «${texto("boot-erro")}»`);
exigir(!ditos.some((r) => r.includes(CONVITE)), "o convite do link foi ao seele.log");
