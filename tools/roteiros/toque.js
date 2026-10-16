// A faixa do toque e o ATENDER (tarefas 24, 26 e 27 da F7; spec de 23/09, «O
// toque» e «Atender, nesta ordem»).
//
// 1. **O toque chama e não toma a tela.** Na entrada, um `Tocou` acende a faixa
//    com o apelido entre aspas e o código, é dito uma vez a quem não vê, e o
//    teclado fica onde estava.
// 2. **RECUSAR** manda o `recusar` de quem toca, e o `Parou` apaga a faixa.
// 3. **ATENDER de dentro do servidor que esta janela hospeda** desmonta só o
//    cliente (o `disconnect`, que desde a tarefa 19 nunca derruba hospedagem) e
//    **nunca** chama o `parar_de_hospedar`, que derrubaria todo mundo que está
//    no servidor; entra na sala pelo `connect` com o alvo que o `atender` deu, e
//    senta na sala de voz.
// 4. **A falha dupla não deixa ninguém sozinho na sala**: a primeira entrada
//    que não chega à sala oferece TENTAR DE NOVO, e a segunda encerra a ligação.
// 5. **A faixa empurra a tela, e não a cobre** (medido pela F3, 15/10): acesa,
//    ela termina antes de começar o conteúdo da tela à vista, na entrada e na
//    sessão; e o veredito de um primeiro contato — a impressão digital que a
//    pessoa compara — não fica embaixo dela.
//
// O duble não tem sala: o roteiro emite os eventos de `seele://toque` e
// responde `atender` e `onde_estou` à mão.

const toque = (carga) => {
  for (const ouvinte of window.__SEELE_OUVINTES["seele://toque"] ?? []) {
    ouvinte({ payload: carga });
  }
};
const RAFA = {
  evento: "Tocou",
  quem: "aa11bb22",
  codigo: "K9P2-CSM7-NBFR-AYQ3",
  apelido: "rafa",
  nome_na_agenda: null,
  com_som: true,
};
const chamados = () => window.__SEELE_HISTORICO.map(([cmd]) => cmd);
// A faixa acesa não cobre nada da tela à vista: ela termina antes de começar o
// conteúdo dela (o topo mais o espaço que a tela abre para ela).
const naoCobre = (tela) => {
  const faixa = document.getElementById("toque").getBoundingClientRect();
  const caixa = document.getElementById(tela);
  const conteudo = caixa.getBoundingClientRect().top + parseFloat(getComputedStyle(caixa).paddingTop);
  relatar(`${tela}: a faixa termina em ${Math.round(faixa.bottom)}, o conteúdo começa em ${Math.round(conteudo)}`);
  exigir(
    faixa.bottom <= conteudo + 0.5,
    `a faixa do toque cobre o topo da #${tela}: ela termina em ${Math.round(faixa.bottom)} e o conteúdo começa em ${Math.round(conteudo)}`,
  );
};
const cruzam = (a, b) => {
  const x = document.getElementById(a).getBoundingClientRect();
  const y = document.getElementById(b).getBoundingClientRect();
  return x.left < y.right && y.left < x.right && x.top < y.bottom && y.top < x.bottom;
};

// 1. Na entrada.
exigir(
  (window.__SEELE_OUVINTES["seele://toque"] ?? []).length > 0,
  "ninguém ouve `seele://toque`: o toque nunca chega à casca",
);
const anunciados = [];
const anunciarDeVerdade = anunciar;
anunciar = (frase) => {
  anunciados.push(frase);
  anunciarDeVerdade(frase);
};
const tecladoAntes = document.activeElement;
toque(RAFA);
await ate(() => visivel("toque") === "VISIVEL", 1000);
exigir(visivel("toque") === "VISIVEL", "o toque chegou e a faixa não apareceu");
const texto = document.getElementById("toque-texto").textContent;
relatar("faixa: " + texto);
exigir(
  texto.includes("«rafa»") && texto.includes("K9P2-CSM7-NBFR-AYQ3"),
  "a faixa não cita o apelido entre aspas com o código: «" + texto + "»",
);
exigir(
  document.activeElement === tecladoAntes,
  "a faixa tomou o teclado de quem estava noutra coisa",
);
naoCobre("tela-boot");
// O `anunciar` escreve num `requestAnimationFrame`, e este navegador sem cabeça
// não roda quadros (medido): o `#anuncio` fica vazio aqui mesmo com a frase
// dita. O que se mede é a chamada.
exigir(
  anunciados.some((frase) => frase.includes("K9P2-CSM7-NBFR-AYQ3")),
  "a faixa apareceu e não disse nada a quem não vê: " + JSON.stringify(anunciados),
);
// **Uma vez por aparição**: a mesma pessoa tocando de novo (outro candidato da
// corrida) redesenha a faixa, e não a diz de novo.
const ditas = anunciados.length;
toque(RAFA);
await espera(200);
exigir(
  anunciados.length === ditas,
  "a faixa foi dita de novo a cada redesenho: " + JSON.stringify(anunciados),
);

// 2. RECUSAR.
document.getElementById("toque-recusar").click();
await ate(() => window.__SEELE_ARGS.recusar?.quem === RAFA.quem, 1000);
exigir(
  window.__SEELE_ARGS.recusar?.quem === RAFA.quem,
  "RECUSAR não mandou o `recusar` de quem toca",
);
toque({ evento: "Parou", quem: RAFA.quem, desfecho: "recusada" });
await ate(() => visivel("toque") === "escondida", 1000);
exigir(visivel("toque") === "escondida", "o toque parou e a faixa ficou");
exigir(
  !document.documentElement.hasAttribute("data-toque-aceso")
    && parseFloat(getComputedStyle(document.getElementById("tela-boot")).paddingTop) === 0,
  "o toque parou e a tela continuou com o espaço da faixa no topo",
);

// 3. ATENDER de dentro do servidor que esta janela hospeda.
await entrarNaSessao();
SEELE_RESPOSTAS.onde_estou = "Guardado";
toque(RAFA);
await ate(() => visivel("toque") === "VISIVEL", 1000);
await ate(() => document.getElementById("toque-atender").textContent.includes("SAI DE"), 1000);
const rotulo = document.getElementById("toque-atender").textContent;
relatar("atender, de dentro do servidor: " + rotulo);
// 5, na sessão: com o veredito de um primeiro contato à vista.
mostrarVeredito({ FirstContact: { fingerprint: "ab12".repeat(16) } });
await espera(100);
naoCobre("tela-sessao");
exigir(
  !cruzam("toque", "veredito"),
  "a faixa do toque cobre o veredito do primeiro contato: a impressão digital fica embaixo dela enquanto toca",
);
exigir(rotulo.includes("SAI DE"), "ATENDER não diz que sai do servidor: «" + rotulo + "»");

SEELE_RESPOSTAS.atender = { ja_na_sala: false, alvo: "127.0.0.1:8385" };
SEELE_RESPOSTAS.onde_estou = "Pessoal";
const antesDoAtender = window.__SEELE_HISTORICO.length;
document.getElementById("toque-atender").click();
await ate(() => chamados().slice(antesDoAtender).includes("enter_voice_room"), 3000);
const ordem = chamados().slice(antesDoAtender);
relatar("ATENDER chamou: " + ordem.join(", "));
exigir(
  !ordem.includes("parar_de_hospedar"),
  "ATENDER chamou o `parar_de_hospedar`, e o servidor que esta janela hospeda caiu com todo mundo dentro",
);
const posicao = (cmd) => ordem.indexOf(cmd);
exigir(
  posicao("atender") >= 0 &&
    posicao("atender") < posicao("disconnect") &&
    posicao("disconnect") < posicao("connect") &&
    posicao("connect") < posicao("enter_voice_room"),
  "ATENDER não seguiu «decidir, desmontar só o cliente, entrar na sala, sentar»: " + ordem.join(", "),
);
exigir(
  window.__SEELE_ARGS.connect?.server === "127.0.0.1:8385",
  "ATENDER entrou em " + window.__SEELE_ARGS.connect?.server + ", e não na sala",
);
exigir(window.__SEELE_HOSPEDANDO === true, "o servidor que esta janela hospeda caiu no ATENDER");
toque({ evento: "Parou", quem: RAFA.quem, desfecho: "atendida" });
await ate(() => visivel("toque") === "escondida", 1000);
exigir(visivel("toque") === "escondida", "a ligação atendida deixou a faixa acesa");

// 4. A falha dupla.
SEELE_RESPOSTAS.onde_estou = "Outro";
toque(RAFA);
await ate(() => visivel("toque") === "VISIVEL", 1000);
document.getElementById("toque-atender").click();
await ate(() => document.getElementById("toque-texto").textContent.includes("NÃO CONSEGUIU"), 3000);
relatar("primeira falha: " + document.getElementById("toque-texto").textContent);
exigir(
  document.getElementById("toque-texto").textContent.includes("NÃO CONSEGUIU"),
  "a entrada que não chegou à sala não foi dita",
);
toque({ evento: "Parou", quem: RAFA.quem, desfecho: "atendida" });
await espera(200);
exigir(
  visivel("toque") === "VISIVEL",
  "o «atendida» apagou a faixa que oferece TENTAR DE NOVO",
);
exigir(
  !chamados().includes("encerrar_a_ligacao"),
  "a primeira falha já encerrou a ligação, sem TENTAR DE NOVO",
);
document.getElementById("toque-atender").click();
await ate(() => chamados().includes("encerrar_a_ligacao"), 3000);
exigir(
  chamados().includes("encerrar_a_ligacao"),
  "a segunda falha não encerrou a ligação, e quem foi atendido fica sozinho na sala",
);
await ate(() => document.getElementById("toque-texto").textContent.includes("ENCERRADA"), 1000);
relatar("segunda falha: " + document.getElementById("toque-texto").textContent);
document.getElementById("toque-recusar").click();
await ate(() => visivel("toque") === "escondida", 1000);
exigir(visivel("toque") === "escondida", "FECHAR não fechou a faixa da ligação encerrada");
