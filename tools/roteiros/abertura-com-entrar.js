// A 0.16 aberta por outro launcher com `--entrar <link>` (tarefa 13 da F6,
// o que tinha ficado de fora do caminho pela recusa).
//
// O `cumprirAAbertura` lê o link do `--entrar`, disca, e o servidor recusa por
// falar o Servidor 0.15. Com o `servidor` presente, a casca oferece abrir o
// SEELE daquela versão (`outra-linha-pelo-launcher.js` prova o resto do
// caminho), e o ABRIR tem de levar **o mesmo link** à outra versão, para ela
// entrar sozinha. Até 21/10/2026 o `conectar` era chamado sem ele, e a outra
// versão abria na tela de entrada dela, com a pessoa tendo de colar o link de
// novo — um link que ela nunca colou, porque quem o deu foi o launcher.
//
// **Reprova** se a abertura não discar, se a recusa não chegar à pergunta, ou
// se o link que chega ao `abrir_versao` não for o do `--entrar`. E se o
// convite do link aparecer numa frase.

const CONVITE = "convite-de-uso-unico-9c1e";
const LINK = "seele://casa.exemplo:8383/?fp=" + "b".repeat(64) + "&convite=" + CONVITE;

const chamou = (cmd) => window.__SEELE_CHAMADAS.includes(cmd);
const texto = (id) => document.getElementById(id)?.textContent ?? "";

SEELE_RESPOSTAS.abertura = { entrar: LINK, hospedar: false, nome_publico: null };
SEELE_RESPOSTAS.analisar_convite = {
  alvo: "casa.exemplo:8383",
  token: CONVITE,
  versao: null,
  para_entrar: "NaoSabe",
};
SEELE_RESPOSTAS.connect = {
  __recusa: {
    error: { OutraLinha: { numero: 8, servidor: "0.15", mais_nova: false } },
    trail: [],
    origem_da_impressao: null,
    a_lista_guardou: false,
  },
};
SEELE_RESPOSTAS.para_entrar_pela_recusa = {
  Abrir: { linha: "0.15", servidor: "0.15", versao: "0.15.2", perde_conversas: false },
};

// A carga já chamou o `cumprirAAbertura` ao abrir a página, quando o duble
// respondia `null` à `abertura`. Agora ela responde o `--entrar`.
window.__SEELE_CHAMADAS.length = 0;
await cumprirAAbertura();
await ate(() => document.getElementById("versoes-confirmar")?.hidden === false, 1500);

relatar(`discou: ${chamou("connect")} · perguntou ao launcher: ${chamou("para_entrar_pela_recusa")} · pergunta: ${visivel("versoes-confirmar")}`);
exigir(chamou("connect"), "a abertura com --entrar não discou");
exigir(visivel("versoes-confirmar") === "VISIVEL",
  "a recusa do Servidor 0.15 na abertura com --entrar não chegou à pergunta da outra versão");

document.getElementById("versoes-confirmar-sim").click();
await ate(() => chamou("abrir_versao"), 1000);
const aberta = window.__SEELE_ARGS.abrir_versao ?? {};
relatar(`abriu: ${chamou("abrir_versao")} · linha ${aberta.linha} · o link do --entrar: ${aberta.link === LINK}`);
exigir(aberta.linha === "0.15", `o ABRIR não abriu o Servidor 0.15: ${JSON.stringify(aberta)}`);
exigir(aberta.link === LINK,
  `o link do --entrar se perdeu no caminho: a outra versão recebeu ${aberta.link}`);
const estado = texto("versoes-confirmar-estado");
relatar(`o estado: «${estado}»`);
exigir(estado.includes("já indo para lá"),
  `com o link do --entrar, o estado não diz que a outra versão já vai para o servidor: «${estado}»`);
const nasFrases = ["boot-erro", "servidores-erro", "versoes-confirmar-frase", "versoes-confirmar-estado"]
  .filter((id) => texto(id).includes(CONVITE));
exigir(nasFrases.length === 0, `o convite do link apareceu em ${nasFrases.join(", ")}`);

// ---- 2. Um `--entrar` cujo `v=` já diz outra linha.
// O `analisar_convite` responde que o servidor é do Servidor 0.15 antes de
// discar (`para_entrar`), e a abertura segue pelo launcher, como o link colado
// no painel faz: pergunta, e **não disca aqui**, porque este app nem fala o
// protocolo daquele servidor. Antes, o `cumprirAAbertura` ignorava a decisão e
// discava, e a pergunta só vinha, se viesse, depois da recusa.
const LINK_DA_0_15 = "seele://casa.exemplo:8383/?fp=" + "c".repeat(64) + "&v=0.15.1";
if (visivel("servidores") === "VISIVEL") document.getElementById("servidores-fechar").click();
SEELE_RESPOSTAS.abertura = { entrar: LINK_DA_0_15, hospedar: false, nome_publico: null };
SEELE_RESPOSTAS.analisar_convite = {
  alvo: "casa.exemplo:8383",
  token: null,
  versao: "0.15.1",
  para_entrar: {
    Abrir: { linha: "0.15", servidor: "0.15", versao: "0.15.2", perde_conversas: false },
  },
};
window.__SEELE_CHAMADAS.length = 0;
delete window.__SEELE_ARGS.abrir_versao;
await cumprirAAbertura();
await ate(() => document.getElementById("versoes-confirmar")?.hidden === false
  && document.getElementById("servidores")?.hidden === false, 1500);
relatar(`v= de outra linha: discou ${chamou("connect")} · pergunta ${visivel("versoes-confirmar")} · painel ${visivel("servidores")}`);
exigir(!chamou("connect"),
  "um --entrar com v= de outra linha discou aqui, num servidor cujo protocolo este app não fala");
exigir(visivel("versoes-confirmar") === "VISIVEL" && visivel("servidores") === "VISIVEL",
  "um --entrar com v= de outra linha não perguntou pela outra versão");
document.getElementById("versoes-confirmar-sim").click();
await ate(() => chamou("abrir_versao"), 1000);
const pelaVersao = window.__SEELE_ARGS.abrir_versao ?? {};
relatar(`v= de outra linha: abriu ${chamou("abrir_versao")} · linha ${pelaVersao.linha} · o link ${pelaVersao.link === LINK_DA_0_15}`);
exigir(pelaVersao.linha === "0.15" && pelaVersao.link === LINK_DA_0_15,
  `o ABRIR não levou o --entrar ao Servidor 0.15: ${JSON.stringify(pelaVersao)}`);
