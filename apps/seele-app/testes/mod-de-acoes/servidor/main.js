// A metade de servidor do MOD de exemplo das ações declaradas (API 7).
//
// **O padrão correto.** O `ModRequest` comum continua valendo para as
// leituras, e por isso o servidor não o veta: quem guarda a operação
// protegida é este handler, e ele só a executa quando `contexto.acao` — que o
// HOST escreve depois de conferir os cargos, e nunca o pedido — é a ação que
// a protege. Nada do corpo do pedido (`pedido.acao`, `pedido.admin`, um
// `contexto` forjado dentro dele) decide coisa nenhuma aqui. E nem o
// `contexto.permissoes`: ele é informativo (para esconder botões), e chega
// também num `ModRequest` comum — guardar a operação por ele deixaria o
// pedido legado executá-la sem passar pelo portão da ação.
//
// O teste de conformidade `acoes_de_mod` carrega este arquivo como está.
globalThis.aoPedir = (contextoJSON, pedidoJSON) => {
  const contexto = JSON.parse(contextoJSON);
  const pedido = JSON.parse(pedidoJSON);
  const fechamentos = Number(dados.fechamentos || '0');
  if (pedido.op === 'ler') {
    return JSON.stringify({ ok: true, mesa: dados.mesa || 'aberta', fechamentos, contexto });
  }
  if (pedido.op === 'fechar') {
    if (contexto.acao !== 'fechar-mesa') {
      return JSON.stringify({ ok: false, erro: 'pede-a-acao-fechar-mesa', contexto });
    }
    dados.mesa = 'fechada';
    dados.fechamentos = String(fechamentos + 1);
    return JSON.stringify({ ok: true, mesa: 'fechada', fechamentos: fechamentos + 1, contexto, pedido });
  }
  return JSON.stringify({ ok: false, erro: 'op-desconhecida', contexto });
};
