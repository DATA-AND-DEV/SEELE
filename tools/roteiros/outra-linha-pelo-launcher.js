// O caminho pela recusa (tarefa 13 da F6, passo 5; P8 da F1; ADR 0058).
//
// Um link sem `v=`, uma entrada da lista ou um endereço digitado: a casca não
// sabe a versão do servidor antes de discar, e o servidor recusa o número deste
// app dizendo, no byte 0 da resposta, qual fala. Com o `servidor` presente (a
// FFI só o manda para uma versão que o launcher abre, o 8 → «0.15»), a casca
// oferece abrir o SEELE daquela versão, com a pergunta das conversas antes,
// como o link com `v=` faz (`versao-do-servidor.js`). A frase da recusa fica:
// ela diz o que houve, e a pergunta diz o que dá para fazer.
//
// **Reprova** se:
// - com o `servidor` presente, a recusa cair só na frase: sem perguntar ao
//   launcher, sem a pergunta, ou sem o `abrir_versao` com a linha e o link;
// - sem o `servidor` (um servidor mais novo que este app, ou de antes da
//   0.15), o launcher for chamado: mais nova quer dizer «atualize»;
// - o convite de uso único do link aparecer no `seele.log` ou numa frase;
// - o foco não for à pergunta, ou não voltar ao CONECTAR quando a pessoa
//   recusa;
// - sem link (a lista, o endereço digitado), o estado prometer que o outro
//   SEELE já vai para o servidor.

const CONVITE = "convite-de-uso-unico-7f3a";
const LINK = "seele://casa.exemplo:8383/?fp=" + "a".repeat(64) + "&convite=" + CONVITE;
const ALVO = "casa.exemplo:8383";

const recusa = (numero, servidor, maisNova) => ({
  __recusa: {
    error: { OutraLinha: { numero, servidor, mais_nova: maisNova } },
    trail: [],
    origem_da_impressao: null,
    a_lista_guardou: false,
  },
});
const ABRIR = {
  Abrir: { linha: "0.15", servidor: "0.15", versao: "0.15.2", perde_conversas: false },
};
const chamou = (cmd) => window.__SEELE_CHAMADAS.includes(cmd);
const texto = (id) => document.getElementById(id)?.textContent ?? "";
const zerar = () => {
  window.__SEELE_CHAMADAS.length = 0;
  window.__SEELE_HISTORICO.length = 0;
  for (const cmd of ["para_entrar_pela_recusa", "abrir_versao", "baixar_versao", "connect"]) {
    delete window.__SEELE_ARGS[cmd];
  }
};
const fecharOPainel = async () => {
  if (visivel("servidores") === "VISIVEL") {
    document.getElementById("servidores-fechar").click();
    await espera(100);
  }
};
// O link colado no painel de servidores, como a pessoa faz.
const colar = async (link) => {
  await fecharOPainel();
  document.getElementById("botao-conectar").click();
  await espera(150);
  document.getElementById("servidores-endereco").value = link;
  zerar();
  document.getElementById("servidores-forma").dispatchEvent(
    new Event("submit", { cancelable: true }),
  );
  await espera(400);
};

// O Rust lê o link: sem `v=`, ele não sabe a versão, e a casca disca aqui.
SEELE_RESPOSTAS.analisar_convite = {
  alvo: ALVO,
  token: CONVITE,
  versao: null,
  para_entrar: "NaoSabe",
};

// ---- 1. O link colado, o Servidor 0.15 guardado aqui.
SEELE_RESPOSTAS.connect = recusa(8, "0.15", false);
SEELE_RESPOSTAS.para_entrar_pela_recusa = ABRIR;
await colar(LINK);

relatar(`discou aqui antes: ${chamou("connect")}`);
exigir(chamou("connect"), "o link sem v= não chegou a discar, e a recusa nunca veio");
const pedida = window.__SEELE_ARGS.para_entrar_pela_recusa;
relatar(`perguntou ao launcher: ${chamou("para_entrar_pela_recusa")} (${JSON.stringify(pedida)})`);
exigir(pedida?.linha === "0.15",
  `a recusa do Servidor 0.15 não perguntou ao launcher pela versão dele: ${JSON.stringify(pedida)}`);
const pergunta = texto("versoes-confirmar-frase");
relatar(`a pergunta: ${visivel("versoes-confirmar")} «${pergunta}»`);
exigir(visivel("servidores") === "VISIVEL" && visivel("versoes-confirmar") === "VISIVEL"
    && pergunta.includes("Servidor 0.15"),
  `a recusa caiu só na frase: servidores ${visivel("servidores")}, ` +
    `versoes-confirmar ${visivel("versoes-confirmar")}, «${pergunta}»`);
relatar(`o foco: ${document.activeElement?.id}`);
exigir(document.activeElement?.id === "versoes-confirmar-sim",
  `a pergunta apareceu sem o foco, que ficou em ${document.activeElement?.id || document.activeElement?.tagName}`);
const frase = texto("boot-erro");
relatar(`a frase da recusa: «${frase.split("\n")[0]}»`);
exigir(frase.startsWith("ESTE SERVIDOR É O SERVIDOR 0.15."),
  `a frase da recusa sumiu ou mudou: «${frase}»`);

document.getElementById("versoes-confirmar-sim").click();
await espera(300);
const aberta = window.__SEELE_ARGS.abrir_versao ?? {};
relatar(`abriu: ${chamou("abrir_versao")} · linha ${aberta.linha} · link inteiro ${aberta.link === LINK} · hospedar ${aberta.hospedar}`);
exigir(chamou("abrir_versao") && aberta.linha === "0.15" && aberta.hospedar === false,
  `o ABRIR não abriu o SEELE do Servidor 0.15 para entrar: ${JSON.stringify(aberta)}`);
exigir(aberta.link === LINK,
  `o SEELE do Servidor 0.15 não recebeu o link inteiro, e não teria como entrar: ${aberta.link}`);

// O convite vai à outra versão, e só a ela: a recusa por versão vem antes do
// Hello, e o convite não foi gasto. Ele não pode ir ao `seele.log` nem a uma
// frase. Os comandos que o levam por ofício são o `analisar_convite`, o
// `connect` e o `abrir_versao`, e o `ler_entrada` (tarefa 30 da F7), que recebe
// o texto colado inteiro para dizer se é convite, endereço ou código, devolve o
// convite como veio, e não guarda nem escreve nada (`seele_core::entrada`).
const vazou = window.__SEELE_HISTORICO
  .filter(([cmd]) => !["analisar_convite", "connect", "abrir_versao", "ler_entrada"].includes(cmd))
  .filter(([, args]) => JSON.stringify(args ?? {}).includes(CONVITE))
  .map(([cmd]) => cmd);
const nasFrases = ["boot-erro", "servidores-erro", "versoes-confirmar-frase", "versoes-confirmar-estado"]
  .filter((id) => texto(id).includes(CONVITE));
relatar(`o convite fora dos comandos que o levam: ${vazou.join(", ") || "nenhum"} · nas frases: ${nasFrases.join(", ") || "nenhuma"}`);
exigir(vazou.length === 0, `o convite do link foi a ${vazou.join(", ")}`);
exigir(nasFrases.length === 0, `o convite do link apareceu em ${nasFrases.join(", ")}`);

// ---- 2. VOLTAR: nada abre, e o foco volta ao CONECTAR, de onde a pessoa veio.
await colar(LINK);
exigir(visivel("versoes-confirmar") === "VISIVEL", "a segunda recusa não perguntou");
document.getElementById("versoes-confirmar-nao").click();
await espera(200);
relatar(`depois do VOLTAR: servidores ${visivel("servidores")} · foco ${document.activeElement?.id} · abriu ${chamou("abrir_versao")}`);
exigir(!chamou("abrir_versao"), "o VOLTAR abriu o SEELE da outra versão");
exigir(visivel("servidores") === "escondida",
  "o VOLTAR deixou o painel de servidores aberto por cima da tela de entrada");
exigir(document.activeElement?.id === "botao-conectar",
  `o VOLTAR não devolveu o foco ao CONECTAR: ficou em ${document.activeElement?.id || document.activeElement?.tagName}`);

// ---- 3. Sem link: uma entrada da lista, ou um endereço digitado.
await fecharOPainel();
zerar();
SEELE_RESPOSTAS.para_entrar_pela_recusa = {
  Baixar: { linha: "0.15", servidor: "0.15", tem_pacote: true, perde_conversas: false },
};
await conectar(ALVO);
await espera(400);
exigir(visivel("versoes-confirmar") === "VISIVEL",
  "a recusa sem link (a lista, o endereço digitado) não perguntou");
document.getElementById("versoes-confirmar-sim").click();
await espera(300);
const semLink = window.__SEELE_ARGS.abrir_versao ?? {};
const estado = texto("versoes-confirmar-estado");
relatar(`sem link: baixou ${chamou("baixar_versao")} · abriu ${chamou("abrir_versao")} · link ${semLink.link} · «${estado}»`);
exigir(chamou("baixar_versao") && chamou("abrir_versao") && semLink.linha === "0.15",
  `sem link, o BAIXAR E ABRIR não baixou e abriu o Servidor 0.15: ${window.__SEELE_CHAMADAS.join(", ")}`);
exigir(semLink.link == null, `sem link colado, a outra versão recebeu um link: ${semLink.link}`);
exigir(!estado.includes("já indo para lá"),
  `sem link, o estado promete que o outro SEELE já vai para o servidor: «${estado}»`);

// ---- 4. Sem o `servidor`: o launcher não é chamado, e a frase decide.
// A resposta do launcher fica armada: se a casca o chamasse, a pergunta
// apareceria, e o roteiro veria.
SEELE_RESPOSTAS.para_entrar_pela_recusa = ABRIR;
for (const [numero, servidor, maisNova, esperada] of [
  [30, null, true, "ESTE SERVIDOR É DE UMA VERSÃO MAIS NOVA DO SEELE."],
  [7, null, false, "ESTE SERVIDOR É DE UMA VERSÃO ANTIGA DO SEELE."],
]) {
  SEELE_RESPOSTAS.connect = recusa(numero, servidor, maisNova);
  await colar(LINK);
  const dita = texto("boot-erro");
  relatar(`${numero}: perguntou ao launcher ${chamou("para_entrar_pela_recusa")} · painel ${visivel("servidores")} · «${dita.split("\n")[0]}»`);
  exigir(!chamou("para_entrar_pela_recusa"),
    `o ${numero} chamou o launcher sem uma versão que ele abre`);
  exigir(visivel("servidores") === "escondida",
    `o ${numero} reabriu o painel de servidores para uma pergunta que não cabe`);
  exigir(dita.startsWith(esperada), `o ${numero} disse «${dita}», e não «${esperada}»`);
}
