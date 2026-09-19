// A metade de servidor do MOD de referência — ADR 0045 e 0049.
//
// Roda no QuickJS do servidor, com `dados` como quintal persistido e
// `aoPedir` / `aoAcontecer` como os dois pontos de entrada.
//
// **O canal vem do contexto, e não do pedido.** O primeiro argumento de
// `aoPedir` é o contexto que o servidor monta — quem pediu, por qual canal, se
// é da administração, se pode escrever —, e é ele quem diz a verdade sobre as
// quatro coisas. O segundo é o que a janela mandou, que é texto de quem pediu.
//
// A bateria **executa este arquivo de verdade**, no mesmo anfitrião que o
// produto usa, e confere a resposta. Ver
// `crates/seele-conformance/tests/mod_de_referencia.rs`.

globalThis.aoPedir = (contexto, pedido) => {
  const quem = JSON.parse(contexto);
  if (quem.channel !== "contar") {
    // Um canal que este MOD não conhece é recusado pelo nome, e não devolvido
    // vazio: quem escreveu a chamada precisa saber por que ela não respondeu.
    return JSON.stringify({ erro: `canal desconhecido: ${quem.channel}` });
  }
  JSON.parse(pedido);
  const vezes = Number(dados.vezes ?? "0") + 1;
  // O quintal guarda texto, e só é gravado se este pedido terminar bem.
  dados.vezes = String(vezes);
  return JSON.stringify({ vezes });
};

globalThis.aoAcontecer = (momento) => {
  dados.ultimo = String(momento?.tipo ?? "");
};
