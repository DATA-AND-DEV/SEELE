/* Um `seele/perfis` 3.3.1 de mentira, da API 5, só para esta pasta (a travessia
   API 5 → API 6, revisão de 25/10). NÃO é o pacote publicado: não tem os bytes
   nem o hash do 3.3.1 oficial, que não está neste repositório. Guarda o perfil
   de cada pessoa no quintal, sob `perfil:<pessoa>`, com o JSON de
   `{ revision, apelido, bio }`. O que a travessia prova é que esse quintal
   chega intacto ao 4.0.0. */
globalThis.aoPedir = (contextoJSON, pedidoJSON) => {
  const contexto = JSON.parse(contextoJSON);
  const pedido = JSON.parse(pedidoJSON);
  const chave = 'perfil:' + contexto.person;
  let perfil = dados[chave] ? JSON.parse(dados[chave]) : { revision: 0, apelido: null, bio: null };
  if (pedido.op === 'save') {
    perfil = { revision: perfil.revision + 1, apelido: pedido.apelido, bio: pedido.bio };
    dados[chave] = JSON.stringify(perfil);
  }
  return JSON.stringify({ ok: true, versao: '3.3.1', ...perfil });
};
