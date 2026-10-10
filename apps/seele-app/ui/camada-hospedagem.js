// SEELE — o servidor que esta janela hospeda, visto da entrada: VOLTAR AO MEU
// SERVIDOR e PARAR DE HOSPEDAR (tarefas 19 e 20 da F7, Plano 2).
//
// **Sair não derruba mais o servidor.** O SAIR, a troca de servidor, o
// RECONECTAR e o ATENDER desmontam só a sessão desta janela, e o servidor
// guardado continua no ar com quem estiver nele. Quem saiu precisa saber disso,
// e precisa de um jeito de voltar e de um jeito de derrubar:
//
// - a frase «VOCÊ CONTINUA HOSPEDANDO …», com o nome que a lista de servidores
//   mostra (`meu_servidor`, no `main.rs`);
// - VOLTAR AO MEU SERVIDOR entra pelo `conectar` de sempre, no endereço desta
//   máquina, na porta em que ele atende;
// - PARAR DE HOSPEDAR pergunta antes, porque derrubar tira todo mundo, e só
//   então chama o `parar_de_hospedar`. Com o servidor vão o link da porta e a
//   dispensa de conferir a própria chave (`nossoServidorEm`, de
//   `tela-boot.js`): um endereço de rede local volta a ser de outra máquina no
//   dia seguinte.
//
// A pergunta «há servidor no ar?» é do Rust, e é feita quando a entrada
// aparece. No navegador, que não hospeda, a ponte responde que não há, e o
// bloco fica escondido.

"use strict";

/** O servidor guardado no ar, como o `meu_servidor` o descreve, ou `null`. */
let meuServidorNoAr = null;

/** O nome que a frase usa: o da lista, ou «o seu servidor». */
function nomeDoMeuServidor() {
  return meuServidorNoAr?.nome || "O SEU SERVIDOR";
}

/** Desenha o bloco, se a entrada está à vista e há servidor no ar. */
async function desenharMeuServidor() {
  const bloco = $("boot-meu-servidor");
  try {
    meuServidorNoAr = await invoke("meu_servidor");
  } catch {
    meuServidorNoAr = null;
  }
  if (!meuServidorNoAr || $("tela-boot").hidden) {
    bloco.hidden = true;
    return;
  }
  $("boot-meu-servidor-frase").textContent =
    `VOCÊ CONTINUA HOSPEDANDO ${nomeDoMeuServidor()}: ele está no ar neste ` +
    "computador, e quem está nele continua lá.";
  bloco.hidden = false;
}

/** Derruba o servidor guardado, depois do sim na caixa. */
async function pararDeHospedarOMeuServidor() {
  try {
    await invoke("parar_de_hospedar");
  } catch (falha) {
    console.warn("parar_de_hospedar:", falha);
  }
  esquecerOLinkDaPorta();
  nossoServidorEm = null;
  await desenharMeuServidor();
}

$("boot-voltar-ao-meu-servidor").addEventListener("click", () => {
  const alvo = meuServidorNoAr?.alvo;
  if (!alvo) return;
  conectar(alvo).catch((falha) => console.warn("VOLTAR AO MEU SERVIDOR:", falha));
});

$("boot-parar-de-hospedar").addEventListener("click", () => {
  const nome = nomeDoMeuServidor();
  abrirConfirmacao(
    "PARAR DE HOSPEDAR",
    `${nome} sai do ar agora, e todo mundo que estiver nele sai junto.\n` +
      "O link de convite dele deixa de levar a algum lugar.",
    `PARAR ${nome}`,
    pararDeHospedarOMeuServidor,
  );
});

// A entrada aparece por mais de um caminho (o SAIR, o fim de uma sessão, a
// troca de servidor que não entrou); o bloco se redesenha quando ela aparece.
new MutationObserver(() => {
  desenharMeuServidor().catch((falha) => console.warn("o meu servidor:", falha));
}).observe($("tela-boot"), { attributes: true, attributeFilter: ["hidden"] });

desenharMeuServidor().catch(() => {});
