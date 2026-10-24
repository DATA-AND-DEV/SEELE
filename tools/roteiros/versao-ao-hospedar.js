// A escolha da versão do servidor mora só dentro de HOSPEDAR (pedido do dono,
// 24/10/2026: «escolher a versão só quando clica em hospedar, dentro daquela
// tela, não na tela inicial»).
//
// **Reprova** se a tela inicial ainda oferecer outra versão; se o HOSPEDAR do
// servidor que já estava aqui subir sem perguntar quando há o Servidor 0.15;
// se o Servidor 0.15 escolhido não abrir a v0.15.2 para hospedar; se o padrão
// não for o Servidor 1.0; ou se um servidor criado nesta versão, ou uma
// máquina com uma versão só, ganhar uma pergunta que não cabe.

const LINHA_1 = { linha: "1", servidor: "1.0", esta: true, instalada: true, tem_pacote: true, apis: [6], perde_conversas: false };
const LINHA_015 = { linha: "0.15", servidor: "0.15", esta: false, instalada: true, tem_pacote: true, apis: [3, 4, 5], perde_conversas: false };
const RAIZ = { id: "raiz", nome: null, caminho: null, versao: null };
const NOVO = { id: "abc", nome: "Novo", caminho: "servidores/abc", versao: "0.16.0" };
SEELE_RESPOSTAS.servidores_guardados = [RAIZ, NOVO];
SEELE_RESPOSTAS.linhas_para_hospedar = [LINHA_1, LINHA_015];
// O `hospedar` recusa, para a página não entrar em sessão entre um caso e
// outro: o roteiro confere só que ele foi chamado, e com qual servidor.
SEELE_RESPOSTAS.hospedar = { __recusa: "NaoSubiu" };

const chamou = (cmd) => window.__SEELE_CHAMADAS.includes(cmd);
const zerar = () => {
  window.__SEELE_CHAMADAS.length = 0;
  for (const cmd of ["hospedar", "abrir_versao"]) delete window.__SEELE_ARGS[cmd];
};
const naTelaInicial = async () => {
  for (const id of ["tela-preparar", "tela-servidores"]) document.getElementById(id).hidden = true;
  document.getElementById("tela-boot").hidden = false;
  await espera(50);
};
const hospedarDaLista = async (nome) => {
  await naTelaInicial();
  zerar();
  document.getElementById("botao-hospedar").click();
  await ate(() => document.getElementById("tela-servidores")?.hidden === false, 1000);
  const linha = [...document.querySelectorAll("#lista-servidores li")]
    .find((li) => li.textContent.includes(nome));
  [...linha.querySelectorAll("button")].find((b) => b.textContent === "HOSPEDAR").click();
  await ate(() => chamou("hospedar") || document.getElementById("tela-preparar")?.hidden === false, 1000);
};

// ---- 1. A tela inicial não oferece outra versão.
const naEntrada = document.getElementById("tela-boot").textContent;
relatar(`tela inicial: boot-versoes ${visivel("boot-versoes")} · «Hospedar com outra versão» ${naEntrada.includes("Hospedar com outra versão")}`);
exigir(visivel("boot-versoes") === "AUSENTE" && !naEntrada.includes("Hospedar com outra versão"),
  "a tela inicial ainda oferece hospedar com outra versão do SEELE");

// ---- 2. O servidor que já estava aqui pergunta, e o Servidor 0.15 abre a v0.15.2.
await hospedarDaLista("O SERVIDOR QUE JÁ ESTAVA AQUI");
relatar(`o que já estava aqui: preparar ${visivel("tela-preparar")} · versões ${visivel("preparar-versoes")} · hospedou ${chamou("hospedar")}`);
exigir(!chamou("hospedar") && visivel("tela-preparar") === "VISIVEL" && visivel("preparar-versoes") === "VISIVEL",
  "o HOSPEDAR do servidor que já estava aqui subiu sem perguntar a versão, havendo o Servidor 0.15");
exigir(visivel("preparar-do-novo") === "escondida",
  "a pergunta da versão do servidor que já estava aqui mostra o nome e a imagem de um servidor novo");
const usar015 = [...document.querySelectorAll("#preparar-lista-versoes li")]
  .find((li) => li.textContent.includes("SERVIDOR 0.15"))?.querySelector("button");
usar015.click();
await espera(100);
document.getElementById("preparar-confirmar").click();
await ate(() => chamou("abrir_versao"), 1000);
const aberta = window.__SEELE_ARGS.abrir_versao ?? {};
relatar(`com o 0.15: abriu ${chamou("abrir_versao")} · linha ${aberta.linha} · hospedar ${aberta.hospedar} · subiu aqui ${chamou("hospedar")}`);
exigir(aberta.linha === "0.15" && aberta.hospedar === true && !chamou("hospedar"),
  `o Servidor 0.15 escolhido não abriu a v0.15.2 para hospedar: ${JSON.stringify(aberta)}`);

// ---- 3. O padrão é o Servidor 1.0: confirmar sem escolher sobe aqui o que já estava.
await hospedarDaLista("O SERVIDOR QUE JÁ ESTAVA AQUI");
document.getElementById("preparar-confirmar").click();
await ate(() => chamou("hospedar"), 1000);
const subiu = window.__SEELE_ARGS.hospedar ?? {};
relatar(`o padrão: subiu aqui ${chamou("hospedar")} · servidor ${subiu.servidor} · abriu outra ${chamou("abrir_versao")}`);
exigir(chamou("hospedar") && subiu.servidor === "raiz" && !chamou("abrir_versao"),
  `sem escolher, o servidor que já estava aqui não subiu no Servidor 1.0: ${JSON.stringify(subiu)}`);

// ---- 4. Um servidor criado nesta versão sobe direto: a v0.15.2 não o tem.
await hospedarDaLista("Novo");
relatar(`o criado na 0.16: subiu ${chamou("hospedar")} · servidor ${window.__SEELE_ARGS.hospedar?.servidor} · preparar ${visivel("tela-preparar")}`);
exigir(chamou("hospedar") && window.__SEELE_ARGS.hospedar?.servidor === "abc",
  "um servidor criado nesta versão ganhou a pergunta da versão, que a v0.15.2 não tem como cumprir");

// ---- 5. Com uma versão só, nada a escolher.
SEELE_RESPOSTAS.linhas_para_hospedar = [LINHA_1];
await hospedarDaLista("O SERVIDOR QUE JÁ ESTAVA AQUI");
relatar(`uma versão só: subiu ${chamou("hospedar")} · preparar ${visivel("tela-preparar")}`);
exigir(chamou("hospedar") && window.__SEELE_ARGS.hospedar?.servidor === "raiz",
  "com uma versão só, o HOSPEDAR do servidor que já estava aqui perguntou uma escolha que não existe");
