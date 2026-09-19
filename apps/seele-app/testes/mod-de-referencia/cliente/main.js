// A metade de janela do MOD de referência — ADR 0049.
//
// Este arquivo é um **vetor do repositório**, e não um exemplo de documentação:
// a bateria o carrega e cobra que cada coisa que ele chama exista do lado do
// produto. Se a API mudar sem que este arquivo mude junto, a bateria reprova
// aqui antes de alguém publicar um guia que promete o que sumiu.
//
// Ele exercita as quatro coisas que a API 3 oferece, e nada além delas:
//
//   SeeleMods.request(id, canal, valor)  — perguntar à metade de servidor
//                                          (canal é um identificador; zero
//                                           quer dizer «nenhum canal»)
//   SeeleMods.snapshot()                 — o estado que a janela já tem
//   SeeleUI.regiao(conteudo)             — desenhar, declarando
//   SeeleUI.tema(valores)                — pedir cor, dentro da sessão
//
// Não há `document`, `window` nem o global do Tauri aqui dentro: um worker não
// os tem, e é isso que faz `terminate()` ser garantia em vez de pedido.

const EU = "seele/referencia";

// **Canal zero: esta pergunta não é sobre um canal.** O terceiro argumento de
// `request` é um identificador de canal, e não o nome de uma operação — a
// operação vai no objeto. Um MOD de escopo de servidor manda zero, e o servidor
// entrega `channel: null` à metade de servidor, sem exigir que haja um canal
// de texto aberto na janela.
const SEM_CANAL = 0;

async function desenhar() {
  const agora = await SeeleMods.snapshot();
  const contagem = await SeeleMods.request(EU, SEM_CANAL, { op: "contar" });

  await SeeleUI.regiao({
    forma: "linha",
    dentro: [
      { forma: "titulo", dentro: "REFERÊNCIA" },
      { forma: "texto", dentro: `sessão: ${agora ? "de pé" : "fora"}` },
      {
        forma: "lista",
        dentro: [
          { forma: "item", dentro: `vezes: ${contagem?.vezes ?? 0}` },
          { forma: "item", dentro: "desenhado pelo produto, declarado por mim" },
        ],
      },
    ],
  });
}

async function comecar() {
  // O tema é pedido uma vez, e o produto o tira sozinho quando este MOD sai.
  await SeeleUI.tema({ acento: "#6BFFB6" });
  await desenhar();
}

comecar();
