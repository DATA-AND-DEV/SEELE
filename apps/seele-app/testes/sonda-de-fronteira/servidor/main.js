// A metade de servidor da sonda: ela só guarda o que a janela mediu.
//
// Existe porque o registro tem de sobreviver à janela. Um resultado que mora
// num `console.warn` de um aplicativo empacotado é um resultado que ninguém lê
// — e este repositório já pagou caro por isso.

globalThis.aoPedir = (contexto, pedido) => {
  const o_que = JSON.parse(pedido);
  if (o_que.op !== "registrar") {
    return JSON.stringify({ erro: `operação desconhecida: ${o_que.op}` });
  }
  // Uma linha por caminho medido, em texto: o quintal guarda texto.
  for (const achado of o_que.achados ?? []) {
    dados[`fronteira:${achado.nome}`] =
      `${achado.alcancou ? "ALCANCOU" : "recusado"}|${achado.detalhe ?? ""}`;
  }
  dados["fronteira:medido-em"] = String(mundo.agora());
  return JSON.stringify({ quantos: (o_que.achados ?? []).length });
};

globalThis.aoAcontecer = () => {};
