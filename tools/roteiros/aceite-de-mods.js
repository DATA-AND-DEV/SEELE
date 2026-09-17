// A tela de aceite de MODs, apertada de verdade — ADR 0045.
//
// O que este roteiro prova, e que nenhum guarda de texto-fonte alcança: que uma
// recusa por MODs **abre o diálogo** em vez de virar linha vermelha, que a
// lista desenhada é a que veio no anúncio, e que ACEITAR grava o sim com a
// identidade certa e tenta entrar de novo.
//
// Era o caminho inteiro que não existia: antes desta tela, o que a pessoa lia
// era uma frase terminando em «a tela para ler e aceitar esta lista ainda não
// existe neste app», e o servidor ficava inentrável.

const ANUNCIO = {
  ModsNaoAceitos: {
    conjunto: "c0ffee1234567890",
    mods: [
      {
        id: "seele/placar",
        version: "1.4.0",
        hash: "aa11bb22cc33dd44ee55ff6677889900aa11bb22cc33dd44ee55ff6677889900",
        repo: "https://example.invalid/placar",
        reach: ["dom", "mensagens do servidor"],
        no_servidor: false,
      },
      {
        id: "ponte/retransmissor",
        version: "0.8.2",
        hash: "bb22cc33dd44ee55ff6677889900aa11bb22cc33dd44ee55ff6677889900aa11",
        repo: "https://example.invalid/ponte",
        reach: ["rede de saída"],
        no_servidor: true,
      },
    ],
  },
};

// O servidor recusa a entrada com a lista na mão.
SEELE_RESPOSTAS.connect = { __recusa: ANUNCIO };
// E esta máquina já disse sim a **outra** lista deste servidor.
SEELE_RESPOSTAS.aceite_de_mods = "uma-lista-diferente";

ultimoAlvo = "203.0.113.9:8383";
await conectar("203.0.113.9:8383", "eu");
await espera(300);

relatar(`diálogo: ${visivel("mods-aceite")}`);
relatar(`itens desenhados: ${document.querySelectorAll("#mods-lista .mods-item").length}`);
relatar(`contagem: ${document.getElementById("mods-contagem").textContent}`);
relatar(`aviso de lista mudada: ${visivel("mods-mudou")}`);
relatar(
  `a linha do que roda no servidor: ${document.querySelectorAll("#mods-lista .mods-servidor").length}`,
);
relatar(`linha vermelha da entrada: ${visivel("boot-erro")}`);

// Aceitar grava e tenta entrar de novo. Desta vez o servidor deixa.
SEELE_RESPOSTAS.connect = { snapshot: SEELE_QUADRO, veredito: null };
window.__SEELE_CHAMADAS.length = 0;
document.getElementById("mods-aceitar").click();
await espera(400);

const gravado = window.__SEELE_ARGS.aceitar_mods ?? {};
relatar(`gravou o sim: alvo=${gravado.alvo} conjunto=${gravado.conjunto}`);
relatar(`tentou entrar de novo: ${window.__SEELE_CHAMADAS.includes("connect")}`);
relatar(`diálogo depois: ${visivel("mods-aceite")}`);
