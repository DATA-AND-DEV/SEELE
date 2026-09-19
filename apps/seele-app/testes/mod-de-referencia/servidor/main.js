// A metade de servidor do MOD de referência — ADR 0045 e 0049.
//
// Roda no QuickJS do servidor, com `dados` como quintal persistido e
// `aoPedir` / `aoAcontecer` como os dois pontos de entrada.
//
// **O contexto é o que o servidor sabe; o pedido é o que a janela diz.** O
// primeiro argumento de `aoPedir` traz quem pediu, por qual canal, se é da
// administração e se pode escrever — e é ele quem diz a verdade sobre as quatro
// coisas. O segundo é texto de quem pediu, e é lá que mora a operação.
//
// **`channel` nulo quer dizer «nenhum canal».** Este MOD é de escopo de
// servidor: o que ele conta não é de canal nenhum, e por isso ele não exige que
// haja um aberto. Um MOD que só faz sentido dentro de um canal confere o
// contrário — que `channel` **não** é nulo.
//
// A bateria executa este arquivo de verdade. Ver
// `crates/seele-conformance/tests/mod_de_referencia.rs`.

globalThis.aoPedir = (contexto, pedido) => {
  const quem = JSON.parse(contexto);
  const o_que = JSON.parse(pedido);

  // **O que a metade de janela viu, guardado onde se pode ler.**
  //
  // A homologação nativa lê o `seele.log` e o `mod_data`, e nenhum dos dois
  // alcança o que aconteceu **dentro** da janela: uma chamada recusada e uma
  // aceita produzem a mesma linha de «fala de MOD nativo», porque as duas são
  // uma mensagem que saiu. Sem isto, «a API foi exercitada» é uma contagem de
  // mensagens fazendo o papel de prova.
  //
  // O quintal é o lugar certo: ele persiste, ele é do MOD, e `sqlite3` o lê
  // sem automação de acessibilidade — que é justamente o que falta nesta
  // máquina.
  if (o_que.op === "anotar") {
    dados[`janela.${String(o_que.chave ?? "?").slice(0, 32)}`] =
      String(o_que.valor ?? "").slice(0, 200);
    return JSON.stringify({ ok: true });
  }

  if (o_que.op !== "contar" && o_que.op !== "gravar") {
    // Uma operação que este MOD não conhece é recusada pelo nome, e não
    // devolvida vazia: quem escreveu a chamada precisa saber por que ela não
    // respondeu.
    return JSON.stringify({ erro: `operação desconhecida: ${o_que.op}` });
  }

  const vezes = Number(dados.vezes ?? "0") + 1;
  // O quintal guarda texto, e só é gravado se este pedido terminar bem.
  dados.vezes = String(vezes);

  if (o_que.op === "gravar") {
    // **A autorização é do servidor, e vem do contexto.** A janela manda o que
    // a pessoa digitou; quem decide se ela pode gravar é esta linha, com o que
    // o servidor sabe — e não um campo que a janela tenha mandado junto.
    //
    // Um MOD que confiasse no pedido estaria deixando quem escreve a chamada
    // escolher a própria permissão.
    if (!quem.write) {
      return JSON.stringify({ erro: "sem permissão de escrita neste servidor" });
    }
    const apelido = String(o_que.apelido ?? "").slice(0, 64);
    dados.apelido = apelido;
    return JSON.stringify({ vezes, apelido, canal: quem.channel });
  }

  return JSON.stringify({ vezes, apelido: dados.apelido ?? "", canal: quem.channel });
};

globalThis.aoAcontecer = (momento) => {
  dados.ultimo = String(momento?.tipo ?? "");
};
