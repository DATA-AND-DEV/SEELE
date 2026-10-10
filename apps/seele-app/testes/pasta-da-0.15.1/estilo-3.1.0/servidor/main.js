/* Um `seele/estilo` 3.1.0 de mentira, só para a pasta da 0.15.1 (tarefa 18 da
   F3). Guarda o tema no quintal como o de verdade guarda — uma chave `theme`
   com o JSON de `{ revision, theme, enabled }` (`servidor/main.js` do ESTILO
   3.1.0, linhas 27 a 34) —, sem a conferência das cores, que não é o assunto. */
globalThis.aoPedir = (contextoJSON, pedidoJSON) => {
  const contexto = JSON.parse(contextoJSON);
  const pedido = JSON.parse(pedidoJSON);
  let estado = dados.theme ? JSON.parse(dados.theme) : { revision: 0, theme: null, enabled: false };
  if (pedido.op === 'save') {
    if (contexto.admin !== true) return JSON.stringify({ ok: false, error: 'só quem administra' });
    estado = { revision: estado.revision + 1, theme: pedido.theme, enabled: true };
    dados.theme = JSON.stringify(estado);
  }
  return JSON.stringify({ ok: true, ...estado });
};
