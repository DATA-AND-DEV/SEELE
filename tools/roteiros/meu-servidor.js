// Sair não derruba o servidor, e a entrada diz como voltar e como parar
// (tarefas 19 e 20 da F7, Plano 2).
//
// 1. **A caixa de sair diz que o servidor continua.** Quem está no próprio
//    servidor e aperta o `+` lê que ele continua no ar, com quem estiver nele,
//    e não que ele «cai junto».
// 2. **Sair desmonta só a sessão.** Chama o `disconnect`, nunca o
//    `parar_de_hospedar`, e o servidor continua hospedado.
// 3. **A entrada diz «VOCÊ CONTINUA HOSPEDANDO»**, com VOLTAR AO MEU SERVIDOR,
//    que entra no endereço desta máquina, e PARAR DE HOSPEDAR.
// 4. **Na sessão de outro servidor, a trilha oferece o VOLTAR**, pela mesma
//    caixa da troca de servidor; dentro do próprio, não.
// 5. **PARAR DE HOSPEDAR pergunta antes**, e só no sim chama o
//    `parar_de_hospedar`; depois o bloco some.
// 6. **A dispensa de conferir a chave é da chave, e não do endereço** (tarefa
//    18 da F7). Entrar no próprio servidor passa direto porque o `onde_estou`
//    reconhece a chave dele; o mesmo endereço com outra chave — outra máquina
//    na mesma rede, no dia seguinte — pede a conferência.
// 7. **HOSPEDAR AQUI, com o servidor daqui no ar, volta a ele** (tarefa 18):
//    não hospeda de novo — o Rust responderia «JÁ ESTOU HOSPEDANDO» — nem abre
//    a tela de preparar.

const chamados = () => window.__SEELE_HISTORICO.map(([cmd]) => cmd);

await entrarNaSessao();
exigir(window.__SEELE_HOSPEDANDO === true, "HOSPEDAR AQUI não deixou um servidor no ar");

// 1. A caixa do `+`, de dentro do próprio servidor.
SEELE_RESPOSTAS.onde_estou = "Guardado";
await pedirAEntrada();
await ate(() => visivel("moderar") === "VISIVEL", 1000);
const frase = document.getElementById("moderar-consequencia").textContent;
relatar("a caixa do +: " + frase.replace(/\s+/g, " "));
exigir(
  frase.includes("continua no ar") && !frase.includes("cai junto"),
  "a caixa de sair do próprio servidor não diz que ele continua no ar: «" + frase + "»",
);

// 2. Sair, pelo sim da caixa.
const antesDeSair = window.__SEELE_HISTORICO.length;
document.getElementById("moderar-confirmar").click();
await ate(() => visivel("tela-boot") === "VISIVEL", 2000);
const aoSair = chamados().slice(antesDeSair);
relatar("sair chamou: " + aoSair.join(", "));
exigir(aoSair.includes("disconnect"), "sair não desmontou a sessão");
exigir(
  !aoSair.includes("parar_de_hospedar"),
  "sair derrubou o servidor que esta janela hospeda, com todo mundo dentro",
);
exigir(window.__SEELE_HOSPEDANDO === true, "o servidor caiu junto com a sessão");

// 3. A entrada diz que ele continua, e oferece voltar.
await ate(() => visivel("boot-meu-servidor") === "VISIVEL", 1500);
exigir(visivel("boot-meu-servidor") === "VISIVEL", "a entrada não diz que o servidor continua no ar");
const aviso = document.getElementById("boot-meu-servidor-frase").textContent;
relatar("a entrada: " + aviso);
exigir(aviso.includes("VOCÊ CONTINUA HOSPEDANDO") && aviso.includes("Casa"), "o aviso não diz o que continua: «" + aviso + "»");

// 6. O mesmo endereço, com outra chave: a conferência aparece.
SEELE_RESPOSTAS.onde_estou = "Outro";
document.getElementById("boot-voltar-ao-meu-servidor").click();
await ate(() => visivel("tela-auth") === "VISIVEL" || visivel("tela-sessao") === "VISIVEL", 2000);
exigir(
  visivel("tela-auth") === "VISIVEL",
  "o endereço do servidor daqui, com a chave de outro, entrou sem conferir: a dispensa ficou no endereço",
);
document.getElementById("auth-voltar").click();
await ate(() => visivel("tela-boot") === "VISIVEL", 2000);
exigir(visivel("tela-boot") === "VISIVEL", "voltar da conferência não levou à entrada");

// 3, com a chave do servidor daqui: entra direto.
delete SEELE_RESPOSTAS.onde_estou;
await ate(() => visivel("boot-meu-servidor") === "VISIVEL", 1500);
const antesDeVoltar = window.__SEELE_HISTORICO.length;
document.getElementById("boot-voltar-ao-meu-servidor").click();
await ate(() => chamados().slice(antesDeVoltar).includes("connect"), 2000);
exigir(
  window.__SEELE_ARGS.connect?.server === "127.0.0.1:8383",
  "VOLTAR AO MEU SERVIDOR entrou em " + window.__SEELE_ARGS.connect?.server + ", e não no servidor daqui",
);
await ate(() => visivel("tela-sessao") === "VISIVEL", 2000);
exigir(visivel("tela-sessao") === "VISIVEL", "VOLTAR AO MEU SERVIDOR não levou à sessão");

// 4. Na sessão de **outro** servidor, a trilha oferece o VOLTAR.
SEELE_RESPOSTAS.onde_estou = "Outro";
await recarregarTrilha();
const voltar = document.querySelector("#trilha-outros .trilha-voltar");
exigir(voltar !== null, "visitando outro servidor, a trilha não oferece VOLTAR AO MEU SERVIDOR");
exigir(voltar.dataset.alvo === "127.0.0.1:8383", "o VOLTAR da trilha leva a " + voltar.dataset.alvo);
voltar.click();
await ate(() => visivel("moderar") === "VISIVEL", 1000);
relatar("a caixa do VOLTAR da trilha: " + document.getElementById("moderar-titulo").textContent);
exigir(
  document.getElementById("moderar-confirmar").textContent.includes("Casa"),
  "a caixa do VOLTAR não diz para onde volta: «" + document.getElementById("moderar-confirmar").textContent + "»",
);
document.getElementById("moderar-cancelar")?.click();
await ate(() => visivel("moderar") === "escondida", 1000);
delete SEELE_RESPOSTAS.onde_estou;
await recarregarTrilha();
exigir(
  document.querySelector("#trilha-outros .trilha-voltar") === null,
  "dentro do próprio servidor, a trilha oferece voltar a ele",
);

// 7. HOSPEDAR AQUI com o servidor daqui no ar.
await sairDoServidorParaAEntrada();
await ate(() => visivel("boot-meu-servidor") === "VISIVEL", 1500);
const antesDoHospedar = window.__SEELE_HISTORICO.length;
document.getElementById("botao-hospedar").click();
await ate(() => visivel("tela-sessao") === "VISIVEL" || visivel("tela-preparar") === "VISIVEL", 2000);
const aoHospedar = chamados().slice(antesDoHospedar);
relatar("HOSPEDAR AQUI com o servidor no ar chamou: " + aoHospedar.join(", "));
exigir(!aoHospedar.includes("hospedar"), "HOSPEDAR AQUI tentou hospedar de novo com o servidor daqui no ar");
exigir(visivel("tela-preparar") !== "VISIVEL", "HOSPEDAR AQUI abriu a tela de preparar com o servidor daqui no ar");
exigir(
  window.__SEELE_ARGS.connect?.server === "127.0.0.1:8383" && visivel("tela-sessao") === "VISIVEL",
  "HOSPEDAR AQUI, com o servidor daqui no ar, não voltou a ele",
);

// 5. Sair de novo, e PARAR DE HOSPEDAR.
await sairDoServidorParaAEntrada();
await ate(() => visivel("boot-meu-servidor") === "VISIVEL", 1500);
document.getElementById("boot-parar-de-hospedar").click();
await ate(() => visivel("moderar") === "VISIVEL", 1000);
exigir(
  !chamados().includes("parar_de_hospedar"),
  "PARAR DE HOSPEDAR derrubou o servidor antes de perguntar",
);
relatar("a caixa de parar: " + document.getElementById("moderar-consequencia").textContent.replace(/\s+/g, " "));
document.getElementById("moderar-confirmar").click();
await ate(() => chamados().includes("parar_de_hospedar"), 1500);
exigir(chamados().includes("parar_de_hospedar"), "o sim da caixa não parou o servidor");
await ate(() => visivel("boot-meu-servidor") === "escondida", 1500);
exigir(visivel("boot-meu-servidor") === "escondida", "o servidor parou e a entrada continua dizendo que ele está no ar");
