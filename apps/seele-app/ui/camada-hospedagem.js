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

// ------------------------------------------------- a confirmação ao fechar (21)
//
// **Fechar o SEELE com o servidor guardado no ar tira todo mundo dele** (tarefa
// 21 da F7; spec de 23/09: «a confirmação ao fechar vale só para o servidor
// guardado»). A sala pessoal não pergunta: ela se despede sozinha, e quem estava
// nela lê «chamada encerrada».
//
// **Um ouvinte só, para todo fechar da janela:** o botão da casca (`close()`
// também pede o fechar), o botão vermelho do macOS e o Alt+F4 do Windows. O
// Cmd+Q do macOS **não** passa por aqui (medido no runtime: o item Quit manda o
// `terminate:`, e o `tao` vai direto ao `RunEvent::Exit`); ele é assunto do
// `main.rs`.
//
// **É a exceção ao guarda da credencial** (lote R da F5, rodada 1, M2), decidida
// pela orquestradora em 10/10 no molde da do `beforeunload`: segurar o fechar
// mantém a página viva, e com ela o que estiver na memória. Por isso, antes de
// qualquer veto, as duas credenciais da tentativa somem: a da pergunta dos MODs
// (`fecharAceiteDeMods()`) e a que um apelido tomado deixou
// (`esquecerARecusaDoApelido()`, de `tela-boot.js`, que é quem a guarda). E o
// veto só acontece com o servidor guardado no ar.

/** A pessoa disse sim: o pedido de fechar seguinte passa direto. */
let fecharSemPerguntar = false;

const janelaDoApp = window.__TAURI__?.window?.getCurrentWindow?.();
janelaDoApp
  ?.onCloseRequested(async (pedido) => {
    if (fecharSemPerguntar) return;
    fecharAceiteDeMods();
    esquecerARecusaDoApelido();
    let meu = null;
    try {
      meu = await invoke("meu_servidor");
    } catch {
      meu = null;
    }
    // Sem servidor no ar, fecha direto: não há o que perguntar.
    if (!meu) return;
    pedido.preventDefault();
    const nome = meu.nome || "O SEU SERVIDOR";
    abrirConfirmacao(
      "FECHAR O SEELE",
      `${nome} sai do ar quando o SEELE fecha, e todo mundo que estiver nele sai junto.\n` +
        "Para sair sem derrubá-lo, saia do servidor e deixe o SEELE aberto.",
      `FECHAR E TIRAR ${nome} DO AR`,
      async () => {
        fecharSemPerguntar = true;
        await janelaDoApp.close();
      },
    );
  })
  .catch((falha) => console.warn("o pedido de fechar:", falha));
