// O SEU CÓDIGO, a agenda, «receber ligações» e a linha da sala (tarefas 23 e 29
// da F7; spec de 23/09, «"SEU CÓDIGO" e a agenda» e «Erros e frases», «Quem
// recebe»).
//
// 1. **A linha da sala só fala quando há o que dizer.** A sala no ar e achável
//    não escreve nada (a entrada e a sessão continuam como eram); a porta 8385
//    ocupada e o código não achável aparecem com a frase de cada um.
// 2. **O PERFIL mostra o SEU CÓDIGO** num campo só de leitura, com COPIAR, e a
//    chave de receber ligações, que manda `receber_ligacoes`.
// 3. **A agenda** mostra quem já ligou, com o nome que eu dei, o apelido entre
//    aspas e o código; BLOQUEAR manda `bloquear`, e um nome novo manda
//    `dar_nome_na_agenda`.
// 4. **O firewall do Windows (D-sala-1)**: com o motivo que o Rust leu, o
//    PERFIL diz o que fazer e para que serve a 8385, e oferece ABRIR A PORTA
//    AGORA **só quando o botão resolve** (`abrir_resolve`): sem regra no SEELE
//    instalado, sim; na cópia portátil, no bloqueio do Windows e sem o
//    instalado, não. Depois da regra criada, um reanúncio do estado velho não
//    devolve o botão.
//
// O duble não tem sala: o roteiro emite `seele://sala` e responde os comandos à
// mão.

const sala = (carga) => {
  for (const ouvinte of window.__SEELE_OUVINTES["seele://sala"] ?? []) {
    ouvinte({ payload: carga });
  }
};
const texto = (id) => document.getElementById(id).textContent.trim();
const CODIGO = "K9P2-CSM7-NBFR-AYQ3";

// 1. A linha da sala.
exigir(
  (window.__SEELE_OUVINTES["seele://sala"] ?? []).length > 0,
  "ninguém ouve `seele://sala`: o estado da sala nunca chega à casca",
);
sala({ estado: "NoAr", registro: { registro: "Registrado" }, firewall: null });
await espera(100);
exigir(
  visivel("boot-sala-estado") === "escondida",
  "a sala no ar e achável escreveu na entrada: «" + texto("boot-sala-estado") + "»",
);
sala({ estado: "Falhou", motivo: "PortaOcupada" });
await ate(() => visivel("boot-sala-estado") === "VISIVEL", 1000);
relatar("porta ocupada: " + texto("boot-sala-estado"));
exigir(
  texto("boot-sala-estado").startsWith("NINGUÉM CONSEGUE TE LIGAR AGORA") &&
    texto("boot-sala-estado").includes("8385"),
  "a porta ocupada não foi dita na entrada: «" + texto("boot-sala-estado") + "»",
);
sala({ estado: "NoAr", registro: { registro: "SemResposta" }, firewall: null });
await ate(() => texto("boot-sala-estado").startsWith("SEU CÓDIGO NÃO ESTÁ ACHÁVEL AGORA"), 1000);
relatar("sem resposta: " + texto("boot-sala-estado"));
exigir(
  texto("boot-sala-estado").startsWith("SEU CÓDIGO NÃO ESTÁ ACHÁVEL AGORA"),
  "o código não achável não foi dito: «" + texto("boot-sala-estado") + "»",
);

// 2. O PERFIL, com o SEU CÓDIGO.
SEELE_RESPOSTAS.meu_codigo = CODIGO;
SEELE_RESPOSTAS.contatos_da_agenda = [
  { codigo: "AAAA-BBBB-CCCC-DDD0", nome: "Rafa", apelido: "rafa", visto_em: 1760000000,
    direcao: "perdida", bloqueado: false },
];
sala({ estado: "NoAr", registro: { registro: "Registrado" },
       firewall: { motivo: "SemRegra", frase: "o firewall do Windows não tem uma regra que libere este SEELE.", abrir_resolve: true } });
document.getElementById("boot-perfil").click();
await ate(() => visivel("perfil-sala") === "VISIVEL", 1500);
exigir(visivel("perfil-sala") === "VISIVEL", "o PERFIL abriu sem o SEU CÓDIGO");
const campo = document.getElementById("perfil-codigo");
exigir(campo.value === CODIGO && campo.readOnly, "o SEU CÓDIGO não está num campo só de leitura");
const receber = document.getElementById("perfil-receber");
exigir(receber.checked, "a sala no ar aparece como «não recebendo»");
receber.click();
await ate(() => window.__SEELE_ARGS.receber_ligacoes !== undefined, 1000);
exigir(
  window.__SEELE_ARGS.receber_ligacoes?.receber === false,
  "desmarcar «receber ligações» não mandou `receber_ligacoes` com falso",
);

// 3. A agenda.
const linha = document.querySelector("#perfil-agenda li");
exigir(linha !== null, "a agenda não desenhou quem já ligou");
relatar("agenda: " + linha.textContent.replace(/\s+/g, " ").trim());
exigir(
  linha.textContent.includes("«rafa»") && linha.textContent.includes("AAAA-BBBB-CCCC-DDD0"),
  "a linha da agenda não mostra o apelido entre aspas e o código",
);
linha.querySelector("button").click();
await ate(() => window.__SEELE_ARGS.bloquear !== undefined, 1000);
exigir(
  window.__SEELE_ARGS.bloquear?.codigo === "AAAA-BBBB-CCCC-DDD0" &&
    window.__SEELE_ARGS.bloquear?.bloqueado === true,
  "BLOQUEAR não mandou `bloquear` do código da linha: " + JSON.stringify(window.__SEELE_ARGS.bloquear),
);
const nome = document.querySelector("#perfil-agenda li input");
nome.value = "Rafael";
nome.dispatchEvent(new Event("change"));
await ate(() => window.__SEELE_ARGS.dar_nome_na_agenda !== undefined, 1000);
exigir(
  window.__SEELE_ARGS.dar_nome_na_agenda?.nome === "Rafael",
  "o nome novo não foi à agenda",
);

// 4. O firewall.
exigir(visivel("perfil-sala-firewall") === "VISIVEL", "o PERFIL não diz que o firewall barra a 8385");
relatar("firewall: " + texto("perfil-sala-firewall"));
exigir(texto("perfil-sala-firewall").includes("8385"), "o aviso do firewall não diz para que serve a 8385");
const botao = () => visivel("perfil-sala-firewall-abrir");
exigir(botao() === "VISIVEL", "sem regra no SEELE instalado, o PERFIL não oferece o botão que resolve");
for (const [motivo, frase] of [
  ["OutraCopia", "o firewall do Windows libera o SEELE de C:\\Program Files\\SEELE\\SEELE.exe, e este é outra cópia. Para receber ligações e hospedar, abra o SEELE instalado."],
  ["Bloqueada", "o firewall do Windows tem uma regra que bloqueia este SEELE («SEELE»). Ela sai em Regras de Entrada."],
  ["SoNoPerfil", "o firewall do Windows libera este SEELE só em rede pública. Quem libera o SEELE no firewall é o instalador."],
]) {
  sala({ estado: "NoAr", registro: { registro: "Registrado" }, firewall: { motivo, frase, abrir_resolve: false } });
  await espera(100);
  exigir(visivel("perfil-sala-firewall") === "VISIVEL", `com ${motivo}, o PERFIL não diz que o firewall barra`);
  exigir(texto("perfil-sala-firewall").includes(frase.slice(0, 40)), `com ${motivo}, o PERFIL não diz o que fazer: «${texto("perfil-sala-firewall")}»`);
  exigir(botao() === "escondida", `com ${motivo}, o PERFIL oferece um botão que não resolve nada`);
}
sala({ estado: "NoAr", registro: { registro: "Registrado" }, firewall: null });
await espera(100);
exigir(visivel("perfil-sala-firewall") === "escondida", "com o firewall liberado, o PERFIL ainda fala dele");

// A regra criada pelo botão: um reanúncio do estado velho não devolve o botão.
const semRegra = { motivo: "SemRegra", frase: "o firewall do Windows não tem uma regra que libere este SEELE.", abrir_resolve: true };
sala({ estado: "NoAr", registro: { registro: "Registrado" }, firewall: semRegra });
await ate(() => botao() === "VISIVEL", 1000);
document.getElementById("perfil-sala-firewall-abrir").click();
await ate(() => texto("perfil-sala-firewall").includes("Regra criada"), 1000);
exigir(window.__SEELE_HISTORICO.some(([cmd]) => cmd === "abrir_a_porta_no_firewall"), "o botão não pediu a regra");
sala({ estado: "NoAr", registro: { registro: "Esperando" }, firewall: semRegra });
await espera(100);
exigir(
  botao() === "escondida" && texto("perfil-sala-firewall").includes("Regra criada"),
  "a regra foi criada, e o reanúncio do estado da sala devolveu o botão e a frase de antes",
);
